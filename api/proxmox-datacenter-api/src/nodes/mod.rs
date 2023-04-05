use anyhow::Error;
use serde_json::{json, Value};

use proxmox_router::{list_subdirs_api_method, Router, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

pub mod tasks;

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_NODES)
    .match_all("node", &ITEM_ROUTER);

pub const ITEM_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
pub const SUBDIRS: SubdirMap = &sorted!([("tasks", &tasks::ROUTER),]);

#[api]
/// List Nodes (only for compatibility)
fn list_nodes() -> Result<Value, Error> {
    Ok(json!([ { "node": proxmox_sys::nodename().to_string() } ]))
}
