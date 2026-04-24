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
        &["metric-collection-total-time"]
    }

    fn set_field(&mut self, name: &str, value: f64) {
        if name == "metric-collection-total-time" {
            self.metric_collection_total_time = Some(value);
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
