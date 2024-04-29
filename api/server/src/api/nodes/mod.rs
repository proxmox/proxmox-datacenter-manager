//! Server/Node Configuration and Administration

use proxmox_router::{list_subdirs_api_method, Router, SubdirMap};
use proxmox_sortable_macro::sortable;

pub mod dns;
pub mod tasks;

use anyhow::Error;
use serde_json::{json, Value};

use proxmox_schema::api;

#[api]
/// List Nodes (only for compatibility)
pub fn list_nodes() -> Result<Value, Error> {
    Ok(json!([ { "node": proxmox_sys::nodename().to_string() } ]))
}

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_NODES)
    .match_all("node", &ITEM_ROUTER);

pub const ITEM_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[rustfmt::skip] // it'll put both entries on 1 line...
#[sortable]
pub const SUBDIRS: SubdirMap = &sorted!([
    ("dns", &dns::ROUTER),
    ("tasks", &tasks::ROUTER),
]);
