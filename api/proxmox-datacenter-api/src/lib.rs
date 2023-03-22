//! Unprivileged PDM API library.

use anyhow::Error;
use serde_json::{json, Value};

use proxmox_router::{list_subdirs_api_method, Permission, Router, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

pub mod remotes;

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("access", &pdm_api_common::api::access::ROUTER),
    ("remotes", &remotes::ROUTER),
    ("ping", &Router::new().get(&API_METHOD_PING)),
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

// FIXME: version should be only accessible to valid user (albeit no specific priv)
#[api(
    access: {
        description: "Anyone can access this.",
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
