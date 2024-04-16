//! Server/Node Configuration and Administration

pub mod dns;

use anyhow::Error;
use serde_json::{json, Value};

use proxmox_schema::api;

#[api]
/// List Nodes (only for compatibility)
pub fn list_nodes() -> Result<Value, Error> {
    Ok(json!([ { "node": proxmox_sys::nodename().to_string() } ]))
}
