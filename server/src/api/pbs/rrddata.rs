use anyhow::Error;
use pdm_api_types::{
    remotes::REMOTE_ID_SCHEMA,
    rrddata::{PbsDatastoreDataPoint, PbsNodeDataPoint},
    PRIV_RESOURCE_AUDIT,
};
use proxmox_router::{Permission, Router};
use proxmox_rrd_api_types::{RrdMode, RrdTimeframe};
use proxmox_schema::api;
use serde_json::Value;

use crate::api::rrd_common::{self, DataPoint};

impl DataPoint for PbsNodeDataPoint {
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
            "cpu_avg1",
            "cpu_avg5",
            "cpu_avg15",
            "disk_total",
            "disk_used",
            "disk_read",
            "disk_write",
            "mem_total",
            "mem_used",
            "swap_total",
            "swap_used",
            "net_in",
            "net_out",
        ]
    }

    fn set_field(&mut self, name: &str, value: f64) {
        match name {
            "cpu_current" => self.cpu_current = Some(value),
            "cpu_iowait" => self.cpu_iowait = Some(value),
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
            _ => {}
        }
    }
}

impl DataPoint for PbsDatastoreDataPoint {
    fn new(time: u64) -> Self {
        Self {
            time,
            ..Default::default()
        }
    }

    fn fields() -> &'static [&'static str] {
        &[
            "disk_total",
            "disk_used",
            "disk_available",
            "disk_read",
            "disk_write",
        ]
    }

    fn set_field(&mut self, name: &str, value: f64) {
        match name {
            "disk_total" => self.disk_total = Some(value),
            "disk_used" => self.disk_used = Some(value),
            "disk_available" => self.disk_available = Some(value),
            "disk_read" => self.disk_read = Some(value),
            "disk_write" => self.disk_write = Some(value),
            _ => {}
        }
    }
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            timeframe: {
                type: RrdTimeframe,
            },
            cf: {
                type: RrdMode,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
        description: "The user needs to have at least the `Resource.Audit` privilege on `/resource/{remote}`."
    },
    returns: {
        type: Array,
        description: "A list of PBS node data points.",
        items: { type: PbsNodeDataPoint },
    },
)]
/// Read PBS node stats
async fn get_pbs_node_rrd_data(
    remote: String,
    timeframe: RrdTimeframe,
    cf: RrdMode,
    _param: Value,
) -> Result<Vec<PbsNodeDataPoint>, Error> {
    let base = format!("pbs/{remote}/host");
    rrd_common::get_rrd_datapoints(remote, base, timeframe, cf).await
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            datastore: { schema: pbs_api_types::DATASTORE_SCHEMA },
            timeframe: {
                type: RrdTimeframe,
            },
            cf: {
                type: RrdMode,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "datastore", "{datastore}"], PRIV_RESOURCE_AUDIT, false),
        description: "The user needs to have at least the `Resource.Audit` privilege on `/resource/{remote}/datastore/{datastore}`."
    },
    returns: {
        type: Array,
        description: "A list of PBS datastore data points.",
        items: { type: PbsDatastoreDataPoint },
    },
)]
/// Read PBS datastore stats
async fn get_pbs_datastore_rrd_data(
    remote: String,
    datastore: String,
    timeframe: RrdTimeframe,
    cf: RrdMode,
    _param: Value,
) -> Result<Vec<PbsDatastoreDataPoint>, Error> {
    let base = format!("pbs/{remote}/datastore/{datastore}");
    rrd_common::get_rrd_datapoints(remote, base, timeframe, cf).await
}

pub const PBS_NODE_RRD_ROUTER: Router = Router::new().get(&API_METHOD_GET_PBS_NODE_RRD_DATA);
pub const PBS_DATASTORE_RRD_ROUTER: Router =
    Router::new().get(&API_METHOD_GET_PBS_DATASTORE_RRD_DATA);
