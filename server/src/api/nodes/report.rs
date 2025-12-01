use anyhow::Error;

use proxmox_router::{Permission, Router};
use proxmox_schema::api;

use pdm_api_types::{NODE_SCHEMA, PRIV_SYS_AUDIT};

use crate::report;

#[api(
    protected: true,
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
        },
    },
    returns: {
        type: String,
        description: "The system report for this PDM node.",
    },
    access: {
        permission: &Permission::Privilege(&["system", "status"], PRIV_SYS_AUDIT, false),
    }
)]
/// Get the system report for this node.
pub fn generate_system_report() -> Result<String, Error> {
    Ok(report::generate_report())
}

pub const ROUTER: Router = Router::new().get(&API_METHOD_GENERATE_SYSTEM_REPORT);
