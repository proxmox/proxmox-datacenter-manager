use anyhow::Error;
use proxmox_rrd_api_types::{RrdMode, RrdTimeframe};

use proxmox_router::Router;
use proxmox_schema::api;

use pdm_api_types::rrddata::PdmNodeDatapoint;

use crate::api::rrd_common::{self, DataPoint};

impl DataPoint for PdmNodeDatapoint {
    fn new(time: u64) -> Self {
        Self {
            time,
            ..Default::default()
        }
    }

    fn fields() -> &'static [&'static str] {
        &["metric-collection-total-time"]
    }

    fn set_field(&mut self, name: &str, value: f64) {
        if name == "metric-collection-total-time" {
            self.total_time = Some(value);
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
        },
    },
)]
/// Read RRD data for this PDM node.
fn get_node_rrddata(timeframe: RrdTimeframe, cf: RrdMode) -> Result<Vec<PdmNodeDatapoint>, Error> {
    let base = "nodes/localhost";
    rrd_common::create_datapoints_from_rrd(base, timeframe, cf)
}

pub const ROUTER: Router = Router::new().get(&API_METHOD_GET_NODE_RRDDATA);
