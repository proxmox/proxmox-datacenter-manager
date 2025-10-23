//! Common API endpoints

use anyhow::Error;
use serde_json::{json, Value};

use proxmox_router::{list_subdirs_api_method, Permission, Router, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

pub mod access;
pub mod config;
pub mod metric_collection;
pub mod nodes;
pub mod pbs;
pub mod pve;
pub mod remote_tasks;
pub mod remote_updates;
pub mod remotes;
pub mod resources;
mod rrd_common;
pub mod sdn;

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("access", &access::ROUTER),
    ("config", &config::ROUTER),
    ("metric-collection", &metric_collection::ROUTER),
    ("ping", &Router::new().get(&API_METHOD_PING)),
    ("pve", &pve::ROUTER),
    ("pbs", &pbs::ROUTER),
    ("remotes", &remotes::ROUTER),
    ("resources", &resources::ROUTER),
    ("nodes", &nodes::ROUTER),
    ("remote-tasks", &remote_tasks::ROUTER),
    // TODO: There might be a better place for this endpoint.
    ("remote-updates", &remote_updates::ROUTER),
    ("sdn", &sdn::ROUTER),
    ("version", &Router::new().get(&API_METHOD_VERSION)),
]);

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[api(
    access: {
        description: "Anyone can access this, just a cheap check if the API daemon is online.",
        permission: &Permission::World,
    }
)]
/// A simple ping method. returns "pong"
fn ping() -> Result<String, Error> {
    Ok("pong".to_string())
}

#[api(
    access: {
        description: "Any valid user can access this.",
        permission: &Permission::Anybody,
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
