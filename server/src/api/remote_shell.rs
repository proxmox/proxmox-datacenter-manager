use anyhow::{bail, format_err, Error};
use serde_json::{json, Value};

use proxmox_auth_api::{
    ticket::{Empty, Ticket},
    Keyring,
};
use proxmox_router::{Permission, RpcEnvironment};
use proxmox_schema::api;

use pdm_api_types::{remotes::REMOTE_ID_SCHEMA, Authid, NODE_SCHEMA, PRIV_SYS_CONSOLE};

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
