use anyhow::Error;

use pdm_api_types::remotes::REMOTE_ID_SCHEMA;
use proxmox_router::{Router, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

pub const ROUTER: Router = Router::new().subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([(
    "trigger",
    &Router::new().post(&API_METHOD_TRIGGER_METRIC_COLLECTION)
),]);

#[api(
    input: {
        properties: {
            remote: {
                schema: REMOTE_ID_SCHEMA,
                optional: true,
            },
        },
    },
)]
/// Trigger metric collection for a provided remote or for all remotes if no remote is passed.
pub async fn trigger_metric_collection(remote: Option<String>) -> Result<(), Error> {
    crate::metric_collection::trigger_metric_collection(remote).await?;

    Ok(())
}
