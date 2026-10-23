use std::fmt;

use anyhow::{Context, Error, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::Instant;

use proxmox_auth_api::{
    Keyring,
    ticket::{Empty, Ticket},
};
use proxmox_client::ApiPathBuilder;
use proxmox_http::websocket::{OpCode, WebSocketError, WebSocketErrorKind, WebSocketWriter};
use proxmox_router::{ApiHandler, ApiMethod, ApiResponseFuture, Permission, RpcEnvironment};
use proxmox_schema::{IntegerSchema, ObjectSchema, StringSchema, api};
use proxmox_sortable_macro::sortable;

use pdm_api_types::{
    Authid, NODE_SCHEMA, PRIV_SYS_CONSOLE,
    remotes::{REMOTE_ID_SCHEMA, Remote, RemoteType},
};

use crate::api::remotes::get_remote;

fn encode_term_ticket_path(remote: &str, node: &str) -> String {
    format!("/shell/{remote}/{node}")
}

#[derive(Clone, Eq, PartialEq, Deserialize, Serialize)]
pub(crate) enum TermTicketType {
    Lxc,
    QemuTerm,
    QemuVnc { ticket: String, port: i64 },
}

impl fmt::Display for TermTicketType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match serde_json::to_string(self) {
            Ok(s) => f.write_str(&s),
            Err(err) => {
                log::error!("error building json string: {err:?}");
                Err(fmt::Error)
            }
        }
    }
}

impl std::str::FromStr for TermTicketType {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        serde_json::from_str(s).context("failed to parse vnc/term ticket information")
    }
}

/// The content of a ticket for a console session proxied to a remote.
pub(crate) trait ConsoleTicket {
    /// Whether the client is a terminal, which can show errors as text.
    fn is_terminal(&self) -> bool;
}

impl ConsoleTicket for TermTicketType {
    fn is_terminal(&self) -> bool {
        !matches!(self, Self::QemuVnc { .. })
    }
}

impl ConsoleTicket for Empty {
    fn is_terminal(&self) -> bool {
        true
    }
}

#[api(
    protected: true,
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
            },
        },
    },
    returns: {
        type: Object,
        description: "Object with the user and ticket",
        properties: {
            user: {
                description: "User that obtained the VNC ticket.",
                type: String,
            },
            ticket: {
                description: "VNC ticket used to authenticate websocket upgrade.",
                type: String,
            },
            port: {
                description: "Always '0'.",
                type: Integer,
            }
        }
    },
    access: {
        description: "Restricted to users",
        permission: &Permission::Privilege(&["resource", "{remote}", "node", "{node}"], PRIV_SYS_CONSOLE, false),
    }
)]
/// Call termproxy and return shell ticket
pub(crate) async fn shell_ticket(
    remote: String,
    node: String,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Value, Error> {
    create_term_ticket(
        rpcenv,
        move || encode_term_ticket_path(&remote, &node),
        || Empty,
    )
}

pub(crate) fn create_term_ticket<T, FPath, FContent>(
    rpcenv: &mut dyn RpcEnvironment,
    make_path: FPath,
    make_content: FContent,
) -> Result<Value, Error>
where
    FPath: FnOnce() -> String,
    FContent: FnOnce() -> T,
    T: ToString + std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Debug,
{
    // intentionally user only for now
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .context("no authid available")?
        .parse()?;

    if auth_id.is_token() {
        bail!("API tokens cannot access this API endpoint");
    }

    let userid = auth_id.user();
    let path = make_path();

    let private_auth_keyring =
        Keyring::with_private_key(crate::auth::key::private_auth_key().clone());

    let content = make_content();
    let ticket = Ticket::new(crate::auth::TERM_PREFIX, &content)?
        .sign(&private_auth_keyring, Some(&format!("{}{}", userid, path)))?;

    Ok(json!({
        "user": userid,
        "ticket": ticket,
        "port": 0,
    }))
}

#[sortable]
pub const API_METHOD_SHELL_WEBSOCKET: ApiMethod = ApiMethod::new(
    &ApiHandler::AsyncHttp(&upgrade_to_websocket),
    &ObjectSchema::new(
        "Upgraded to websocket",
        &sorted!([
            ("remote", false, &REMOTE_ID_SCHEMA),
            ("node", false, &NODE_SCHEMA),
            (
                "vncticket",
                false,
                &StringSchema::new("Terminal ticket").schema()
            ),
            ("port", false, &IntegerSchema::new("Terminal port").schema()),
        ]),
    ),
)
.access(
    Some("The user needs Sys.Console on /resource/{remote}/node/{node}."),
    &Permission::Privilege(
        &["resource", "{remote}", "node", "{node}"],
        PRIV_SYS_CONSOLE,
        false,
    ),
);

fn upgrade_to_websocket(
    parts: http::request::Parts,
    req_body: hyper::body::Incoming,
    param: Value,
    _info: &ApiMethod,
    rpcenv: Box<dyn RpcEnvironment>,
) -> ApiResponseFuture {
    Box::pin(upgrade_to_websocket_do(parts, req_body, param, rpcenv))
}

/// Show the client why the terminal session on the remote could not be set up.
///
/// Until the session is established, the terminal client waits for the "OK" which termproxy sends
/// after verifying the ticket and closes the connection on anything else, so the message has to
/// follow an "OK" to be shown at all.
pub(crate) async fn send_setup_error<S>(stream: &mut S, err: &Error)
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut ws = WebSocketWriter::new(None, &mut *stream);

    let msg = format!("{err:#}").replace('\n', "\r\n");
    let msg = format!("OK\r\nfailed to open the terminal on the remote: {msg}\r\n");

    if let Err(err) = ws.write_all(msg.as_bytes()).await {
        log::debug!("could not send terminal setup error to client: {err}");
        return;
    }
    // The details are in the terminal, the close reason only has to fit into a control frame.
    let close = WebSocketError::new(WebSocketErrorKind::Unexpected, "remote connection failed");
    if let Err(err) = ws
        .send_control_frame(None, OpCode::Close, &close.generate_frame_payload())
        .await
    {
        log::debug!("could not close terminal websocket: {err}");
        return;
    }

    // Closing with unread data, such as the client's pings during a slow setup or its close reply,
    // resets the connection, which can discard the error before the client read it. So read and
    // drop what the client sends for a moment, bounded so a chatty client cannot keep it open.
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut buf = [0u8; 1024];
    loop {
        let idle = Instant::now() + Duration::from_millis(500);
        match tokio::time::timeout_at(idle.min(deadline), stream.read(&mut buf)).await {
            Ok(Ok(len)) if len > 0 => continue,
            _ => break,
        }
    }
}

/// Making this a generic function instead currently runs into the issue that
/// `get_ticket_and_port` would need to be an `AsyncFnOnce` where `T::CallOnceFuture: Send`, which
/// can currently only be expressed with nightly rust.
///
/// Still - there is so much code here that it is worth de-duplicating via a macro.
///
/// All parameters (except for the name and `$ticket_type`, which must implement
/// [`ConsoleTicket`]) are functions/closures.
/// The output of `$get_params` flows into the rest.
/// The output of `$get_ticket_and_port` should be a tuple `(ticket, port, P2, use_preamble)` where
/// `P2` is extra data which flows into `$make_vncwebsocket_path`, and `use_preamble` tells whether
/// the remote expects termproxy's `user:ticket` line before the session data.
///
/// The signatures are as follows:
/// ```ignore
/// $get_params: fn(param: &Value) -> Result<P1, Error>;
/// $make_ticket_path: fn(remote: &str, args1: &P1) -> String;
/// $get_ticket_and_port: async fn (
///     remote: &Remote,
///     args1: &P1,
///     ticket_data: $ticket_type,
/// ) -> Result<(String, i64, P2, bool), Error>;
/// $make_vncwebsocket_path: fn (args1: P1, args2: P2, ticket: &str, port: i64) -> String;
/// ```
macro_rules! upgrade_to_websocket_impl {
    (
        $name:ident,
        $get_params:expr,
        $make_ticket_path:expr,
        $ticket_type:ty,
        $get_ticket_and_port:expr,
        $make_vncwebsocket_path:expr,
    ) => {
        async fn $name(
            parts: ::http::request::Parts,
            req_body: ::hyper::body::Incoming,
            param: ::serde_json::Value,
            rpcenv: Box<dyn RpcEnvironment>,
        ) -> Result<::http::Response<::proxmox_http::Body>, Error> {
            use ::proxmox_auth_api::ticket::Ticket;
            use ::proxmox_http::{Body, websocket::WebSocket};

            // intentionally user only for now
            let auth_id: Authid = rpcenv
                .get_auth_id()
                .context("no authid available")?
                .parse()?;

            if auth_id.is_token() {
                bail!("API tokens cannot access this API endpoint");
            }

            let userid = auth_id.user();
            let ticket =
                crate::api::nodes::vncwebsocket::required_string_param(&param, "vncticket")?;

            let public_auth_keyring = ::proxmox_auth_api::Keyring::with_public_key(
                crate::auth::key::public_auth_key().clone(),
            );

            let remote = crate::api::nodes::vncwebsocket::required_string_param(&param, "remote")?
                .to_owned();
            let shell_params = $get_params(&param)?;
            let ticket_path = $make_ticket_path(&remote, &shell_params);

            let ticket_data = Ticket::<$ticket_type>::parse(ticket)?.verify(
                &public_auth_keyring,
                crate::auth::TERM_PREFIX,
                Some(&format!("{}{}", userid, ticket_path)),
            )?;

            let (mut ws, response) = WebSocket::new(parts.headers.clone())?;

            proxmox_rest_server::spawn_internal_task(async move {
                use futures::TryFutureExt;

                let incoming_ws =
                    match hyper::upgrade::on(http::Request::from_parts(parts, req_body))
                        .map_err(Error::from)
                        .await
                    {
                        Ok(upgraded) => upgraded,
                        _ => bail!("error"),
                    };
                let mut incoming_ws = hyper_util::rt::TokioIo::new(incoming_ws);
                let is_terminal =
                    crate::api::remotes::shell::ConsoleTicket::is_terminal(&ticket_data);

                let connect_remote = async {
                    let (remotes, _digest) = pdm_config::remotes::config()?;
                    let remote = get_remote(&remotes, &remote)?;
                    let (ticket, port, further_args, use_preamble) =
                        $get_ticket_and_port(&remote, &shell_params, ticket_data).await?;

                    let raw_client = crate::connection::make_raw_client(remote)?;

                    let ws_key = proxmox_sys::linux::random_data(16)?;
                    let ws_key = proxmox_base64::encode(&ws_key);

                    let api_url = raw_client.api_url().clone().into_parts();

                    let mut builder = http::uri::Builder::new();
                    if let Some(scheme) = api_url.scheme {
                        builder = builder.scheme(scheme);
                    }
                    if let Some(authority) = api_url.authority {
                        builder = builder.authority(authority)
                    }
                    let api_path =
                        $make_vncwebsocket_path(shell_params, further_args, &ticket, port);
                    let uri = builder
                        .path_and_query(api_path)
                        .build()
                        .map_err(|err| ::anyhow::format_err!("failed to build Uri - {err}"))?;

                    let auth = raw_client.login_auth()?;
                    let req = http::Request::builder()
                        .method(http::Method::GET)
                        .uri(uri)
                        .header(http::header::UPGRADE, "websocket")
                        .header(http::header::SEC_WEBSOCKET_VERSION, "13")
                        .header(http::header::SEC_WEBSOCKET_KEY, ws_key);

                    let req = auth.set_auth_headers(req).body(Body::empty())?;

                    let res = raw_client.http_client().request(req).await?;
                    if res.status() != http::StatusCode::SWITCHING_PROTOCOLS {
                        bail!("server didn't upgrade: {}", res.status());
                    }

                    let pve_ws = hyper::upgrade::on(res)
                        .await
                        .map_err(|err| ::anyhow::format_err!("failed to upgrade - {}", err))?;

                    let username =
                        if let proxmox_client::AuthenticationKind::Token(ref token) = *auth {
                            token.userid.clone()
                        } else {
                            bail!("shell not supported with ticket-based authentication")
                        };

                    let preamble = if use_preamble {
                        format!("{username}:{ticket}\n", ticket = ticket)
                    } else {
                        String::new()
                    };

                    Ok::<_, Error>((pve_ws, preamble))
                };

                let (pve_ws, preamble) = match connect_remote.await {
                    Ok(connection) => connection,
                    Err(err) => {
                        log::warn!("could not open console on remote '{remote}': {err:#}");
                        if is_terminal {
                            crate::api::remotes::shell::send_setup_error(&mut incoming_ws, &err)
                                .await;
                        }
                        return Err(err);
                    }
                };

                ws.mask = Some([0, 0, 0, 0]);

                if let Err(err) = ws
                    .proxy_connection(
                        incoming_ws,
                        hyper_util::rt::TokioIo::new(pve_ws),
                        preamble.as_bytes(),
                    )
                    .await
                {
                    log::warn!("error while copying between websockets: {err:?}");
                }

                Ok(())
            });

            Ok(response)
        }
    };
}

pub(crate) use upgrade_to_websocket_impl;

upgrade_to_websocket_impl! {
    upgrade_to_websocket_do,
    |param: &Value| {
        crate::api::nodes::vncwebsocket::required_string_param(param, "node")
            .map(str::to_owned)
    },
    encode_term_ticket_path,
    Empty,
    async |remote: &Remote, node: &String, _: Empty| -> Result<(String, i64, (), bool), Error> {
        Ok(match remote.ty {
            RemoteType::Pve => {
                let pve = crate::connection::make_pve_client(remote)?;
                let pve_term_ticket = pve
                    .node_shell_termproxy(
                        node,
                        pve_api_types::NodeShellTermproxy {
                            cmd: None,
                            cmd_opts: None,
                        },
                    )
                    .await?;
                (pve_term_ticket.ticket, pve_term_ticket.port, (), true)
            }
            RemoteType::Pbs => {
                let pbs = crate::connection::make_pbs_client(remote)?;
                let pbs_term_ticket = pbs.node_shell_termproxy().await?;
                (pbs_term_ticket.ticket, pbs_term_ticket.port as i64, (), true)
            }
        })
    },
    (|node: String, _: (), ticket: &str, port: i64| {
        ApiPathBuilder::new(format!("/api2/json/nodes/{node}/vncwebsocket"))
            .arg("vncticket", ticket)
            .arg("port", port)
            .build()
    }),
}
