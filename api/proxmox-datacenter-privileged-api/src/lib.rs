//! Privileged PDM API library.

use anyhow::Error;
use serde_json::{json, Value};

use proxmox_router::{list_subdirs_api_method, Permission, Router, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

pub mod auth;

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("access", &pdm_api_common::api::access::ROUTER),
    ("version", &Router::new().get(&API_METHOD_VERSION)),
]);

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[api(
    access: {
        // only root-user can access privileged daemon locally, so make it easier to query by avoid
        // requiring a ticket.
        description: "Anyone that can access the privileged daemon can access this.",
        permission: &Permission::World,
    }
)]
/// Return the program's version/release info
fn version() -> Result<Value, Error> {
    Ok(json!({
        "version": pdm_buildcfg::PROXMOX_PKG_VERSION,
        "release": pdm_buildcfg::PROXMOX_PKG_RELEASE,
        "repoid": pdm_buildcfg::PROXMOX_PKG_REPOID
    }))
}
