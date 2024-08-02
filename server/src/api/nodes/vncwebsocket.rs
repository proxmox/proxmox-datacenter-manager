//! Server/Node Configuration and Administration

use anyhow::{bail, format_err, Error};
use futures::future::{FutureExt, TryFutureExt};
use hyper::body::Body;
use hyper::http::request::Parts;
use hyper::upgrade::Upgraded;
use hyper::Request;
use serde_json::Value;

use proxmox_auth_api::ticket::{Empty, Ticket};
use proxmox_auth_api::types::Authid;
use proxmox_auth_api::Keyring;

use proxmox_http::websocket::WebSocket;
use proxmox_router::{
    ApiHandler, ApiMethod, ApiResponseFuture, Permission, Router, RpcEnvironment,
};
use proxmox_schema::*;
use proxmox_sortable_macro::sortable;

use pdm_api_types::{NODE_SCHEMA, PRIV_SYS_CONSOLE};

pub fn required_string_param<'a>(param: &'a Value, name: &str) -> Result<&'a str, Error> {
    match param[name].as_str() {
        Some(s) => Ok(s),
        None => bail!("missing parameter '{}'", name),
    }
}

pub fn required_integer_param(param: &Value, name: &str) -> Result<i64, Error> {
    match param[name].as_i64() {
        Some(s) => Ok(s),
        None => bail!("missing parameter '{}'", name),
    }
}

pub const SHELL_CMD_SCHEMA: Schema = StringSchema::new("The command to run.")
    .format(&ApiStringFormat::Enum(&[
        EnumEntry::new("login", "Login"),
        EnumEntry::new("upgrade", "Upgrade"),
    ]))
    .schema();

#[sortable]
pub const API_METHOD_WEBSOCKET: ApiMethod = ApiMethod::new(
    &ApiHandler::AsyncHttp(&upgrade_to_websocket),
    &ObjectSchema::new(
        "Upgraded to websocket",
        &sorted!([
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
    Some("The user needs Sys.Console on /system."),
    &Permission::Privilege(&["system"], PRIV_SYS_CONSOLE, false),
);

fn upgrade_to_websocket(
    parts: Parts,
    req_body: Body,
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
        let port: u16 = required_integer_param(&param, "port")? as u16;

        let public_auth_keyring =
            Keyring::with_public_key(crate::auth::key::public_auth_key().clone());

        // will be checked again by termproxy
        Ticket::<Empty>::parse(ticket)?.verify(
            &public_auth_keyring,
            crate::auth::TERM_PREFIX,
            Some(&format!("{}{}{}", userid, "/system", port)),
        )?;

        let (ws, response) = WebSocket::new(parts.headers.clone())?;

        proxmox_rest_server::spawn_internal_task(async move {
            let conn: Upgraded = match hyper::upgrade::on(Request::from_parts(parts, req_body))
                .map_err(Error::from)
                .await
            {
                Ok(upgraded) => upgraded,
                _ => bail!("error"),
            };

            let local = tokio::net::TcpStream::connect(format!("localhost:{}", port)).await?;
            ws.serve_connection(conn, local).await
        });

        Ok(response)
    }
    .boxed()
}

pub const ROUTER: Router = Router::new().upgrade(&API_METHOD_WEBSOCKET);
