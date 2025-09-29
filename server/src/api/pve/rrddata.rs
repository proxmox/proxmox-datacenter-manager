use anyhow::Error;
use serde_json::Value;

use proxmox_router::{Permission, Router};
use proxmox_rrd_api_types::{RrdMode, RrdTimeframe};
use proxmox_schema::api;

use pdm_api_types::remotes::REMOTE_ID_SCHEMA;
use pdm_api_types::rrddata::{LxcDataPoint, NodeDataPoint, PveStorageDataPoint, QemuDataPoint};
use pdm_api_types::{NODE_SCHEMA, PRIV_RESOURCE_AUDIT, PVE_STORAGE_ID_SCHEMA, VMID_SCHEMA};

use crate::api::rrd_common::{self, DataPoint};

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

impl DataPoint for PveStorageDataPoint {
    fn new(time: u64) -> Self {
        Self {
            time,
            ..Default::default()
        }
    }

    fn fields() -> &'static [&'static str] {
        &["disk_total", "disk_used"]
    }

    fn set_field(&mut self, name: &str, value: f64) {
        match name {
            "disk_total" => self.disk_total = Some(value),
            "disk_used" => self.disk_used = Some(value),
            _ => {}
        }
    }
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
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Read qemu stats
async fn get_qemu_rrd_data(
    remote: String,
    vmid: u32,
    timeframe: RrdTimeframe,
    cf: RrdMode,
    _param: Value,
) -> Result<Vec<QemuDataPoint>, Error> {
    let base = format!("pve/{remote}/qemu/{vmid}");
    rrd_common::get_rrd_datapoints(remote, base, timeframe, cf).await
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
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Read lxc stats
async fn get_lxc_rrd_data(
    remote: String,
    vmid: u32,
    timeframe: RrdTimeframe,
    cf: RrdMode,
    _param: Value,
) -> Result<Vec<LxcDataPoint>, Error> {
    let base = format!("pve/{remote}/lxc/{vmid}");
    rrd_common::get_rrd_datapoints(remote, base, timeframe, cf).await
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
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "node", "{node}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Read node stats
async fn get_node_rrd_data(
    remote: String,
    node: String,
    timeframe: RrdTimeframe,
    cf: RrdMode,
    _param: Value,
) -> Result<Vec<NodeDataPoint>, Error> {
    let base = format!("pve/{remote}/node/{node}");
    rrd_common::get_rrd_datapoints(remote, base, timeframe, cf).await
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: { schema: NODE_SCHEMA },
            storage: { schema: PVE_STORAGE_ID_SCHEMA },
            timeframe: {
                type: RrdTimeframe,
            },
            cf: {
                type: RrdMode,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "storage", "{storage}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Read node stats
async fn get_storage_rrd_data(
    remote: String,
    node: String,
    storage: String,
    timeframe: RrdTimeframe,
    cf: RrdMode,
    _param: Value,
) -> Result<Vec<NodeDataPoint>, Error> {
    let base = format!("pve/{remote}/storage/{node}/{storage}");
    rrd_common::get_rrd_datapoints(remote, base, timeframe, cf).await
}

pub const QEMU_RRD_ROUTER: Router = Router::new().get(&API_METHOD_GET_QEMU_RRD_DATA);
pub const LXC_RRD_ROUTER: Router = Router::new().get(&API_METHOD_GET_LXC_RRD_DATA);
pub const NODE_RRD_ROUTER: Router = Router::new().get(&API_METHOD_GET_NODE_RRD_DATA);
pub const STORAGE_RRD_ROUTER: Router = Router::new().get(&API_METHOD_GET_STORAGE_RRD_DATA);
