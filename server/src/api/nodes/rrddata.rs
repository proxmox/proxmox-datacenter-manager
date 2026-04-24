use anyhow::Error;
use proxmox_rrd_api_types::{RrdMode, RrdTimeframe};

use proxmox_router::{http_bail, Router};
use proxmox_schema::api;

use pdm_api_types::rrddata::PdmNodeDatapoint;
use pdm_api_types::NODE_SCHEMA;

use crate::api::rrd_common::{self, DataPoint};

impl DataPoint for PdmNodeDatapoint {
    fn new(time: u64) -> Self {
        Self {
            time,
            ..Default::default()
        }
    }

    fn fields() -> &'static [&'static str] {
        &[
            "cpu-current",
            "cpu-iowait",
            "cpu-avg1",
            "cpu-avg5",
            "cpu-avg15",
            "cpu-pressure-some-avg10",
            "disk-total",
            "disk-used",
            "disk-read-iops",
            "disk-write-iops",
            "disk-read",
            "disk-write",
            "disk-io-ticks",
            "io-pressure-some-avg10",
            "io-pressure-full-avg10",
            "mem-total",
            "mem-used",
            "mem-pressure-some-avg10",
            "mem-pressure-full-avg10",
            "swap-total",
            "swap-used",
            "net-in",
            "net-out",
            "metric-collection-total-time",
        ]
    }

    fn set_field(&mut self, name: &str, value: f64) {
        match name {
            "cpu-current" => self.cpu_current = Some(value),
            "cpu-iowait" => self.cpu_iowait = Some(value),
            "cpu-avg1" => self.cpu_avg1 = Some(value),
            "cpu-avg5" => self.cpu_avg5 = Some(value),
            "cpu-avg15" => self.cpu_avg15 = Some(value),
            "cpu-pressure-some-avg10" => self.cpu_pressure_some_avg10 = Some(value),
            "disk-total" => self.disk_total = Some(value),
            "disk-used" => self.disk_used = Some(value),
            "disk-read-iops" => self.disk_read_iops = Some(value),
            "disk-write-iops" => self.disk_write_iops = Some(value),
            "disk-read" => self.disk_read = Some(value),
            "disk-write" => self.disk_write = Some(value),
            "disk-io-ticks" => self.disk_io_ticks = Some(value),
            "io-pressure-some-avg10" => self.io_pressure_some_avg10 = Some(value),
            "io-pressure-full-avg10" => self.io_pressure_full_avg10 = Some(value),
            "mem-total" => self.mem_total = Some(value),
            "mem-used" => self.mem_used = Some(value),
            "mem-pressure-some-avg10" => self.mem_pressure_some_avg10 = Some(value),
            "mem-pressure-full-avg10" => self.mem_pressure_full_avg10 = Some(value),
            "swap-total" => self.swap_total = Some(value),
            "swap-used" => self.swap_used = Some(value),
            "net-in" => self.net_in = Some(value),
            "net-out" => self.net_out = Some(value),
            "metric-collection-total-time" => self.metric_collection_total_time = Some(value),
            _ => log::error!("setting invalid field '{name}' in PdmNodeDatapoint"),
        }
    }
}

#[api(
    input: {
        properties: {
            timeframe: {
                type: RrdTimeframe,
            },
            cf: {
                type: RrdMode,
            },
            node: {
                schema: NODE_SCHEMA,
            },
        },
    },
    returns: {
        type: Array,
        description: "An array of RRD data points.",
        items: {
            type: PdmNodeDatapoint,
        }
    }
)]
/// Read RRD data for this PDM node.
fn get_node_rrddata(
    node: String,
    timeframe: RrdTimeframe,
    cf: RrdMode,
) -> Result<Vec<PdmNodeDatapoint>, Error> {
    if node != "localhost" {
        http_bail!(
            BAD_REQUEST,
            "PDM only supports `localhost` as a `node` parameter"
        );
    }
    let base = "nodes/localhost";
    rrd_common::create_datapoints_from_rrd(base, timeframe, cf)
}

pub const ROUTER: Router = Router::new().get(&API_METHOD_GET_NODE_RRDDATA);
