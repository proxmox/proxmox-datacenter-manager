use anyhow::Error;

use proxmox_router::{list_subdirs_api_method, Permission, Router, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use pdm_api_types::remotes::REMOTE_ID_SCHEMA;
use pdm_api_types::{NODE_SCHEMA, PRIV_RESOURCE_AUDIT, PVE_STORAGE_ID_SCHEMA};

use super::connect_to_remote;

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(STORAGE_SUBDIR))
    .subdirs(STORAGE_SUBDIR);
#[sortable]
const STORAGE_SUBDIR: SubdirMap = &sorted!([
    ("rrddata", &super::rrddata::STORAGE_RRD_ROUTER),
    ("status", &Router::new().get(&API_METHOD_GET_STATUS)),
]);

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: { schema: NODE_SCHEMA, },
            storage: { schema: PVE_STORAGE_ID_SCHEMA, },
        },
    },
    returns: { type: pve_api_types::QemuStatus },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "storage", "{storage}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get the status of a qemu VM from a remote. If a node is provided, the VM must be on that
/// node, otherwise the node is determined automatically.
pub async fn get_status(
    remote: String,
    node: String,
    storage: String,
) -> Result<pve_api_types::StorageStatus, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

    Ok(pve.storage_status(&node, &storage).await?)
}
