//! Server/Node Configuration and Administration

use proxmox_router::{list_subdirs_api_method, Router, SubdirMap};
use proxmox_sortable_macro::sortable;

pub mod apt;
pub mod certificates;
pub mod config;
pub mod dns;
pub mod journal;
pub mod network;
pub mod rrddata;
pub mod status;
pub mod syslog;
pub mod tasks;
pub mod termproxy;
pub mod time;
pub mod vncwebsocket;

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
    ("apt", &apt::ROUTER),
    ("certificates", &certificates::ROUTER),
    ("config", &config::ROUTER),
    ("dns", &dns::ROUTER),
    ("journal", &journal::ROUTER),
    ("network", &network::ROUTER),
    ("rrdata", &rrddata::ROUTER),
    ("status", &status::ROUTER),
    ("syslog", &syslog::ROUTER),
    ("tasks", &tasks::ROUTER),
    ("termproxy", &termproxy::ROUTER),
    ("time", &time::ROUTER),
    ("vncwebsocket", &vncwebsocket::ROUTER),
]);
