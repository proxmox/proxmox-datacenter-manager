use anyhow::Error;

use proxmox_router::{list_subdirs_api_method, Router, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use pdm_api_types::{remotes::REMOTE_ID_SCHEMA, NODE_SCHEMA};

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("rrddata", &super::rrddata::NODE_RRD_ROUTER),
    ("network", &Router::new().get(&API_METHOD_GET_NETWORK))
]);

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: { schema: NODE_SCHEMA },
            "interface-type": {
                type: pve_api_types::ListNetworksType,
                optional: true,
            },
        },
    },
)]
/// Get network interfaces from PVE node
async fn get_network(
    remote: String,
    node: String,
    interface_type: Option<pve_api_types::ListNetworksType>,
) -> Result<Vec<pve_api_types::NetworkInterface>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let client = super::connect_to_remote(&remotes, &remote)?;
    let networks = client.list_networks(&node, interface_type).await?;
    Ok(networks)
}
