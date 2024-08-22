use std::collections::BTreeMap;

use anyhow::{bail, Error};
use serde_json::{json, Value};

use proxmox_router::Router;
use proxmox_rrd::api_types::{RRDMode, RRDTimeFrame};
use proxmox_schema::api;

use pdm_api_types::remotes::REMOTE_ID_SCHEMA;
use pdm_api_types::{NODE_SCHEMA, VMID_SCHEMA};

use crate::metric_collection::rrd_cache;

fn create_value_from_rrd(
    basedir: &str,
    list: &[&str],
    timeframe: RRDTimeFrame,
    mode: RRDMode,
) -> Result<Value, Error> {
    let mut result: Vec<Value> = Vec::new();

    let mut timemap = BTreeMap::new();

    let mut last_resolution = None;

    for name in list {
        let (start, reso, data) = match rrd_cache::extract_data(basedir, name, timeframe, mode)? {
            Some(result) => result.into(),
            None => continue,
        };

        if let Some(expected_resolution) = last_resolution {
            if reso != expected_resolution {
                bail!(
                    "got unexpected RRD resolution ({} != {})",
                    reso,
                    expected_resolution
                );
            }
        } else {
            last_resolution = Some(reso);
        }

        let mut t = start;

        for value in data {
            let entry = timemap.entry(t).or_insert_with(|| json!({ "time": t }));
            if let Some(value) = value {
                entry[*name] = value.into();
            }
            t += reso;
        }
    }

    for item in timemap.values() {
        result.push(item.clone());
    }

    Ok(result.into())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            vmid: { schema: VMID_SCHEMA },
            timeframe: {
                type: RRDTimeFrame,
            },
            cf: {
                type: RRDMode,
            },
        },
    },
)]
/// Read qemu stats
fn get_qemu_rrd_data(
    remote: String,
    vmid: u32,
    timeframe: RRDTimeFrame,
    cf: RRDMode,
    _param: Value,
) -> Result<Value, Error> {
    let base = format!("pve/{remote}/qemu/{vmid}");

    create_value_from_rrd(
        &base,
        &[
            "cpu_current",
            "cpu_max",
            "disk_read",
            "disk_write",
            "disk_total",
            "mem_total",
            "mem_used",
            "net_in",
            "net_out",
            "uptime",
        ],
        timeframe,
        cf,
    )
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            vmid: { schema: VMID_SCHEMA },
            timeframe: {
                type: RRDTimeFrame,
            },
            cf: {
                type: RRDMode,
            },
        },
    },
)]
/// Read lxc stats
fn get_lxc_rrd_data(
    remote: String,
    vmid: u32,
    timeframe: RRDTimeFrame,
    cf: RRDMode,
    _param: Value,
) -> Result<Value, Error> {
    let base = format!("pve/{remote}/lxc/{vmid}");

    create_value_from_rrd(
        &base,
        &[
            "cpu_current",
            "cpu_max",
            "disk_read",
            "disk_write",
            "disk_total",
            "disk_used",
            "mem_total",
            "mem_used",
            "mem_used",
            "net_in",
            "net_out",
            "uptime",
        ],
        timeframe,
        cf,
    )
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: { schema: NODE_SCHEMA },
            timeframe: {
                type: RRDTimeFrame,
            },
            cf: {
                type: RRDMode,
            },
        },
    },
)]
/// Read node stats
fn get_node_rrd_data(
    remote: String,
    node: String,
    timeframe: RRDTimeFrame,
    cf: RRDMode,
    _param: Value,
) -> Result<Value, Error> {
    let base = format!("pve/{remote}/node/{node}");

    create_value_from_rrd(
        &base,
        &[
            "cpu_current",
            "cpu_iowait",
            "cpu_max",
            "cpu_avg1",
            "cpu_avg5",
            "cpu_avg15",
            "disk_total",
            "disk_used",
            "mem_total",
            "mem_used",
            "swap_total",
            "swap_used",
            "mem_used",
            "net_in",
            "net_out",
            "uptime",
        ],
        timeframe,
        cf,
    )
}
pub const QEMU_RRD_ROUTER: Router = Router::new().get(&API_METHOD_GET_QEMU_RRD_DATA);
pub const LXC_RRD_ROUTER: Router = Router::new().get(&API_METHOD_GET_LXC_RRD_DATA);
pub const NODE_RRD_ROUTER: Router = Router::new().get(&API_METHOD_GET_NODE_RRD_DATA);