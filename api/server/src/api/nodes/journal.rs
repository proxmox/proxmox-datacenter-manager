use std::process::{Command, Stdio};

use anyhow::Error;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};

use proxmox_router::{ApiMethod, Permission, Router, RpcEnvironment};
use proxmox_schema::api;

use proxmox_syslog_api::{dump_journal, JournalFilter};

use pdm_api_types::{NODE_SCHEMA, PRIV_SYS_AUDIT};

#[api(
    protected: true,
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
            filter: {
                type: JournalFilter,
                flatten: true,
            },
        },
    },
    returns: {
        type: Array,
        description: "Returns a list of journal entries.",
        items: {
            type: String,
            description: "Line text.",
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "log"], PRIV_SYS_AUDIT, false),
    },
)]
/// Read syslog entries.
#[allow(clippy::too_many_arguments)]
fn get_journal(
    filter: JournalFilter,
    _param: Value,
    _info: &ApiMethod,
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<String>, Error> {
    dump_journal(filter)
}

pub const ROUTER: Router = Router::new().get(&API_METHOD_GET_JOURNAL);
