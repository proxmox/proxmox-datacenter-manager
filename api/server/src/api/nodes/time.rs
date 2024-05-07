use anyhow::Error;
use serde_json::Value;

use proxmox_router::{Permission, Router};
use proxmox_schema::api;

use pdm_api_types::{NODE_SCHEMA, PRIV_SYS_MODIFY, TIME_ZONE_SCHEMA};
use proxmox_system_config_api::ServerTimeInfo;

#[api(
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
        },
    },
    returns: {
        type: ServerTimeInfo,
    },
    access: {
        permission: &Permission::Anybody,
    },
)]
/// Read server time and time zone settings.
fn get_time(_param: Value) -> Result<ServerTimeInfo, Error> {
    proxmox_system_config_api::time::get_server_time_info()
}

#[api(
    protected: true,
    reload_timezone: true,
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
            timezone: {
                schema: TIME_ZONE_SCHEMA,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "time"], PRIV_SYS_MODIFY, false),
    },
)]
/// Set time zone
fn set_timezone(timezone: String, _param: Value) -> Result<(), Error> {
    proxmox_system_config_api::time::set_timezone(timezone)
}

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_GET_TIME)
    .put(&API_METHOD_SET_TIMEZONE);
