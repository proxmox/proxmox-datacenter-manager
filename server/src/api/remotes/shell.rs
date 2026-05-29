use anyhow::{Context, Error, bail};
use serde_json::{Value, json};

use proxmox_auth_api::{
    Keyring,
    ticket::{Empty, Ticket},
};
use proxmox_client::ApiPathBuilder;
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
    create_term_ticket(rpcenv, move || encode_term_ticket_path(&remote, &node))
}

pub(crate) fn create_term_ticket<F: FnOnce() -> String>(
    rpcenv: &mut dyn RpcEnvironment,
    make_path: F,
) -> Result<Value, Error> {
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

    let ticket = Ticket::new(crate::auth::TERM_PREFIX, &Empty)?
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

/// Making this a generic function instead currently runs into the issue that
/// `get_ticket_and_port` would need to be an `AsyncFnOnce` where `T::CallOnceFuture: Send`, which
/// can currently only be expressed with nightly rust.
///
/// Still - there is so much code here that it is worth de-duplicating via a macro.
///
/// All parameters (except for the name) are functions/closures.
/// The output of `$get_params` flows into the rest.
/// The output of `$get_ticket_and_port` should be a tuple `(ticket, port, P)` where `P` is extra
/// data which flows into `$make_vncwebsocket_path`.
///
/// The signatures are as follows:
/// ```ignore
/// $get_params: fn(param: &Value) -> Result<P1, Error>;
/// $make_ticket_path: fn(remote: &Remote, args1: &P1) -> String;
/// $get_ticket_and_port: async fn (remote: &Remote, args1: &P1) -> Result<(String, i64, P2), Error>;
/// $make_vncwebsocket_path: fn (args1: P1, args2: P2, ticket: &str, port: i64) -> String;
/// ```
macro_rules! upgrade_to_websocket_impl {
    ($name:ident, $get_params:expr, $make_ticket_path:expr, $get_ticket_and_port:expr, $make_vncwebsocket_path:expr) => {
        async fn $name(
            parts: ::http::request::Parts,
            req_body: ::hyper::body::Incoming,
            param: ::serde_json::Value,
            rpcenv: Box<dyn RpcEnvironment>,
        ) -> Result<::http::Response<::proxmox_http::Body>, Error> {
            use ::proxmox_auth_api::ticket::{Empty, Ticket};
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

            Ticket::<Empty>::parse(ticket)?.verify(
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

                let (remotes, _digest) = pdm_config::remotes::config()?;
                let remote = get_remote(&remotes, &remote)?;
                let (ticket, port, further_args) =
                    $get_ticket_and_port(&remote, &shell_params).await?;

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
                let api_path = $make_vncwebsocket_path(shell_params, further_args, &ticket, port);
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

                let username = if let proxmox_client::AuthenticationKind::Token(ref token) = *auth {
                    token.userid.clone()
                } else {
                    bail!("shell not supported with ticket-based authentication")
                };

                let preamble = format!("{username}:{ticket}\n", ticket = ticket);
                ws.mask = Some([0, 0, 0, 0]);

                if let Err(err) = ws
                    .proxy_connection(
                        hyper_util::rt::TokioIo::new(incoming_ws),
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
    async |remote: &Remote, node: &String| -> Result<(String, i64, ()), Error> {
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
                (pve_term_ticket.ticket, pve_term_ticket.port, ())
            }
            RemoteType::Pbs => {
                let pbs = crate::connection::make_pbs_client(remote)?;
                let pbs_term_ticket = pbs.node_shell_termproxy().await?;
                (pbs_term_ticket.ticket, pbs_term_ticket.port as i64, ())
            }
        })
    },
    (|node: String, _: (), ticket: &str, port: i64| {
        ApiPathBuilder::new(format!("/api2/json/nodes/{node}/vncwebsocket"))
            .arg("vncticket", ticket)
            .arg("port", port)
            .build()
    })
}
