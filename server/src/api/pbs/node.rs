use anyhow::Error;

use proxmox_router::{list_subdirs_api_method, Permission, Router, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use pdm_api_types::remotes::REMOTE_ID_SCHEMA;
use pdm_api_types::PRIV_RESOURCE_AUDIT;

use pbs_api_types::NODE_SCHEMA;

use crate::connection;
use crate::pbs_client::get_remote;

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("apt", &crate::api::remote_updates::APT_ROUTER),
    (
        "subscription",
        &Router::new().get(&API_METHOD_GET_SUBSCRIPTION)
    ),
    (
        "termproxy",
        &Router::new().post(&crate::api::remote_shell::API_METHOD_SHELL_TICKET)
    ),
    (
        "vncwebsocket",
        &Router::new().upgrade(&crate::api::remote_shell::API_METHOD_SHELL_WEBSOCKET)
    ),
]);

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: { schema: NODE_SCHEMA }, // not used, always localhost
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
        description: "The user needs to have at least the `Resource.Audit` privilege on `/resource/{remote}`."
    },
    returns: { type: proxmox_subscription::SubscriptionInfo }
)]
/// Get subscription for the PBS remote
async fn get_subscription(remote: String) -> Result<proxmox_subscription::SubscriptionInfo, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let remote = get_remote(&remotes, &remote)?;
    Ok(connection::make_pbs_client(remote)?
        .get_subscription()
        .await?)
}
