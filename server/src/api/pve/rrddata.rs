use std::collections::BTreeMap;

use anyhow::{bail, Error};
use serde_json::Value;

use proxmox_router::Router;
use proxmox_rrd_api_types::{RrdMode, RrdTimeframe};
use proxmox_schema::api;

use pdm_api_types::remotes::REMOTE_ID_SCHEMA;
use pdm_api_types::rrddata::{LxcDataPoint, NodeDataPoint, QemuDataPoint};
use pdm_api_types::{NODE_SCHEMA, VMID_SCHEMA};

use crate::metric_collection::rrd_cache;

trait DataPoint {
    /// Create a new  data point with a given timestamp
    fn new(time: u64) -> Self;
    /// Returns the names of the underlying (stringly typed) fields in the RRD
    fn fields() -> &'static [&'static str];
    /// Set a member by its field identifier
    fn set_field(&mut self, name: &str, value: f64);
}

impl DataPoint for NodeDataPoint {
    fn new(time: u64) -> Self {
        Self {
            time,
            ..Default::default()
        }
    }

    fn fields() -> &'static [&'static str] {
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
            "net_in",
            "net_out",
            "uptime",
        ]
    }

    fn set_field(&mut self, name: &str, value: f64) {
        match name {
            "cpu_current" => self.cpu_current = Some(value),
            "cpu_iowait" => self.cpu_iowait = Some(value),
            "cpu_max" => self.cpu_max = Some(value),
            "cpu_avg1" => self.cpu_avg1 = Some(value),
            "cpu_avg5" => self.cpu_avg5 = Some(value),
            "cpu_avg15" => self.cpu_avg15 = Some(value),
            "disk_total" => self.disk_total = Some(value),
            "disk_used" => self.disk_used = Some(value),
            "mem_total" => self.mem_total = Some(value),
            "mem_used" => self.mem_used = Some(value),
            "swap_total" => self.swap_total = Some(value),
            "swap_used" => self.swap_used = Some(value),
            "net_in" => self.net_in = Some(value),
            "net_out" => self.net_out = Some(value),
            "uptime" => self.uptime = Some(value),
            _ => {}
        }
    }
}

impl DataPoint for QemuDataPoint {
    fn new(time: u64) -> Self {
        Self {
            time,
            ..Default::default()
        }
    }

    fn fields() -> &'static [&'static str] {
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
        ]
    }

    fn set_field(&mut self, name: &str, value: f64) {
        match name {
            "cpu_current" => self.cpu_current = Some(value),
            "cpu_max" => self.cpu_max = Some(value),
            "disk_read" => self.disk_read = Some(value),
            "disk_write" => self.disk_write = Some(value),
            "disk_total" => self.disk_total = Some(value),
            "mem_total" => self.mem_total = Some(value),
            "mem_used" => self.mem_used = Some(value),
            "net_in" => self.net_in = Some(value),
            "net_out" => self.net_out = Some(value),
            "uptime" => self.uptime = Some(value),
            _ => {}
        }
    }
}

impl DataPoint for LxcDataPoint {
    fn new(time: u64) -> Self {
        Self {
            time,
            ..Default::default()
        }
    }

    fn fields() -> &'static [&'static str] {
        &[
            "cpu_current",
            "cpu_max",
            "disk_read",
            "disk_write",
            "disk_total",
            "disk_used",
            "mem_total",
            "mem_used",
            "net_in",
            "net_out",
            "uptime",
        ]
    }

    fn set_field(&mut self, name: &str, value: f64) {
        match name {
            "cpu_current" => self.cpu_current = Some(value),
            "cpu_max" => self.cpu_max = Some(value),
            "disk_read" => self.disk_read = Some(value),
            "disk_write" => self.disk_write = Some(value),
            "disk_total" => self.disk_total = Some(value),
            "disk_used" => self.disk_used = Some(value),
            "mem_total" => self.mem_total = Some(value),
            "mem_used" => self.mem_used = Some(value),
            "net_in" => self.net_in = Some(value),
            "net_out" => self.net_out = Some(value),
            "uptime" => self.uptime = Some(value),
            _ => {}
        }
    }
}

fn create_datapoints_from_rrd<T: DataPoint>(
    basedir: &str,
    timeframe: RrdTimeframe,
    mode: RrdMode,
) -> Result<Vec<T>, Error> {
    let mut timemap = BTreeMap::new();
    let mut last_resolution = None;

    for name in T::fields() {
        let (start, resolution, data) =
            match rrd_cache::extract_data(basedir, name, timeframe, mode)? {
                Some(data) => data.into(),
                None => continue,
            };

        if let Some(expected_resolution) = last_resolution {
            if resolution != expected_resolution {
                bail!("got unexpected RRD resolution ({resolution} != {expected_resolution})",);
            }
        } else {
            last_resolution = Some(resolution);
        }

        let mut t = start;

        for value in data {
            let entry = timemap.entry(t).or_insert_with(|| T::new(t));
            if let Some(value) = value {
                entry.set_field(name, value);
            }

            t += resolution;
        }
    }

    Ok(timemap.into_values().collect())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            vmid: { schema: VMID_SCHEMA },
            timeframe: {
                type: RrdTimeframe,
            },
            cf: {
                type: RrdMode,
            },
        },
    },
)]
/// Read qemu stats
fn get_qemu_rrd_data(
    remote: String,
    vmid: u32,
    timeframe: RrdTimeframe,
    cf: RrdMode,
    _param: Value,
) -> Result<Vec<QemuDataPoint>, Error> {
    let base = format!("pve/{remote}/qemu/{vmid}");

    create_datapoints_from_rrd(&base, timeframe, cf)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            vmid: { schema: VMID_SCHEMA },
            timeframe: {
                type: RrdTimeframe,
            },
            cf: {
                type: RrdMode,
            },
        },
    },
)]
/// Read lxc stats
fn get_lxc_rrd_data(
    remote: String,
    vmid: u32,
    timeframe: RrdTimeframe,
    cf: RrdMode,
    _param: Value,
) -> Result<Vec<LxcDataPoint>, Error> {
    let base = format!("pve/{remote}/lxc/{vmid}");

    create_datapoints_from_rrd(&base, timeframe, cf)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: { schema: NODE_SCHEMA },
            timeframe: {
                type: RrdTimeframe,
            },
            cf: {
                type: RrdMode,
            },
        },
    },
)]
/// Read node stats
fn get_node_rrd_data(
    remote: String,
    node: String,
    timeframe: RrdTimeframe,
    cf: RrdMode,
    _param: Value,
) -> Result<Vec<NodeDataPoint>, Error> {
    let base = format!("pve/{remote}/node/{node}");

    create_datapoints_from_rrd(&base, timeframe, cf)
}
pub const QEMU_RRD_ROUTER: Router = Router::new().get(&API_METHOD_GET_QEMU_RRD_DATA);
pub const LXC_RRD_ROUTER: Router = Router::new().get(&API_METHOD_GET_LXC_RRD_DATA);
pub const NODE_RRD_ROUTER: Router = Router::new().get(&API_METHOD_GET_NODE_RRD_DATA);
