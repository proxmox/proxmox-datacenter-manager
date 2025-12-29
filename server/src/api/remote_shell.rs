use anyhow::{bail, format_err, Error};
use futures::{FutureExt, TryFutureExt};
use http::{
    header::{SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_VERSION, UPGRADE},
    request::Parts,
    Method, Request, StatusCode,
};
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};

use proxmox_auth_api::{
    ticket::{Empty, Ticket},
    Keyring,
};
use proxmox_client::ApiPathBuilder;
use proxmox_http::{websocket::WebSocket, Body};
use proxmox_router::{ApiHandler, ApiMethod, ApiResponseFuture, Permission, RpcEnvironment};
use proxmox_schema::{api, IntegerSchema, ObjectSchema, StringSchema};
use proxmox_sortable_macro::sortable;

use pdm_api_types::{
    remotes::{RemoteType, REMOTE_ID_SCHEMA},
    Authid, NODE_SCHEMA, PRIV_SYS_CONSOLE,
};

use crate::api::{nodes::vncwebsocket::required_string_param, remotes::get_remote};

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
    // intentionally user only for now
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .ok_or_else(|| format_err!("no authid available"))?
        .parse()?;

    if auth_id.is_token() {
        bail!("API tokens cannot access this API endpoint");
    }

    let userid = auth_id.user();
    let path = encode_term_ticket_path(&remote, &node);

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
    parts: Parts,
    req_body: hyper::body::Incoming,
    param: Value,
    _info: &ApiMethod,
    rpcenv: Box<dyn RpcEnvironment>,
) -> ApiResponseFuture {
    async move {
        // intentionally user only for now
        let auth_id: Authid = rpcenv
            .get_auth_id()
            .ok_or_else(|| format_err!("no authid available"))?
            .parse()?;

        if auth_id.is_token() {
            bail!("API tokens cannot access this API endpoint");
        }

        let userid = auth_id.user();
        let ticket = required_string_param(&param, "vncticket")?;

        let public_auth_keyring =
            Keyring::with_public_key(crate::auth::key::public_auth_key().clone());

        let remote = required_string_param(&param, "remote")?.to_owned();
        let node = required_string_param(&param, "node")?.to_owned();
        let ticket_path = encode_term_ticket_path(&remote, &node);

        Ticket::<Empty>::parse(ticket)?.verify(
            &public_auth_keyring,
            crate::auth::TERM_PREFIX,
            Some(&format!("{}{}", userid, ticket_path)),
        )?;

        let (mut ws, response) = WebSocket::new(parts.headers.clone())?;

        proxmox_rest_server::spawn_internal_task(async move {
            let incoming_ws: Upgraded =
                match hyper::upgrade::on(Request::from_parts(parts, req_body))
                    .map_err(Error::from)
                    .await
                {
                    Ok(upgraded) => upgraded,
                    _ => bail!("error"),
                };

            let (remotes, _digest) = pdm_config::remotes::config()?;
            let remote = get_remote(&remotes, &remote)?;
            let (ticket, port) = match remote.ty {
                RemoteType::Pve => {
                    let pve = crate::connection::make_pve_client(remote)?;
                    let pve_term_ticket = pve
                        .node_shell_termproxy(
                            &node,
                            pve_api_types::NodeShellTermproxy {
                                cmd: None,
                                cmd_opts: None,
                            },
                        )
                        .await?;
                    (pve_term_ticket.ticket, pve_term_ticket.port)
                }
                RemoteType::Pbs => {
                    let pbs = crate::connection::make_pbs_client(remote)?;
                    let pbs_term_ticket = pbs.node_shell_termproxy().await?;
                    (pbs_term_ticket.ticket, pbs_term_ticket.port as i64)
                }
            };

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
            let api_path = ApiPathBuilder::new(format!("/api2/json/nodes/{node}/vncwebsocket"))
                .arg("vncticket", ticket.clone())
                .arg("port", port)
                .build();
            let uri = builder
                .path_and_query(api_path)
                .build()
                .map_err(|err| format_err!("failed to build Uri - {err}"))?;

            let auth = raw_client.login_auth()?;
            let req = Request::builder()
                .method(Method::GET)
                .uri(uri)
                .header(UPGRADE, "websocket")
                .header(SEC_WEBSOCKET_VERSION, "13")
                .header(SEC_WEBSOCKET_KEY, ws_key);

            let req = auth.set_auth_headers(req).body(Body::empty())?;

            let res = raw_client.http_client().request(req).await?;
            if res.status() != StatusCode::SWITCHING_PROTOCOLS {
                bail!("server didn't upgrade: {}", res.status());
            }

            let pve_ws = hyper::upgrade::on(res)
                .await
                .map_err(|err| format_err!("failed to upgrade - {}", err))?;

            let username = if let proxmox_client::AuthenticationKind::Token(ref token) = *auth {
                token.userid.clone()
            } else {
                bail!("shell not supported with ticket-based authentication")
            };

            let preamble = format!("{username}:{ticket}\n", ticket = ticket);
            ws.mask = Some([0, 0, 0, 0]);

            if let Err(err) = ws
                .proxy_connection(
                    TokioIo::new(incoming_ws),
                    TokioIo::new(pve_ws),
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
    .boxed()
}
