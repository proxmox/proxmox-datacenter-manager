use anyhow::Error;
use serde_json::Value;

use proxmox_router::{ApiMethod, Permission, Router, RpcEnvironment};
use proxmox_schema::api;

use proxmox_syslog_api::{dump_syslog, SyslogFilter, SyslogLine};

use pdm_api_types::{NODE_SCHEMA, PRIV_SYS_AUDIT};

#[api(
    protected: true,
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
            filter: {
                type: SyslogFilter,
                flatten: true,
            },
        },
    },
    returns: {
        type: Array,
        description: "Returns a list of syslog entries.",
        items: {
            type: SyslogLine,
        }
    },
    access: {
        permission: &Permission::Privilege(&["system", "log"], PRIV_SYS_AUDIT, false),
    },
)]
/// Read syslog entries.
fn get_syslog(
    filter: SyslogFilter,
    _info: &ApiMethod,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<SyslogLine>, Error> {
    //filter.service = filter.service.map(crate::api2::node::services::real_service_name);

    let (count, lines) = dump_syslog(filter)?;

    rpcenv["total"] = Value::from(count);

    Ok(lines)
}

pub const ROUTER: Router = Router::new().get(&API_METHOD_GET_SYSLOG);
