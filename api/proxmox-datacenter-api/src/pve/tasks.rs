//! Access to PVE tasks.

use anyhow::{bail, format_err, Error};

use proxmox_router::{list_subdirs_api_method, Router, RpcEnvironment, SubdirMap};
use proxmox_schema::{api, Schema};
use proxmox_sortable_macro::sortable;

use pdm_api_types::{Remote, RemoteUpid, REMOTE_ID_SCHEMA};
use pve_client::types::PveUpid;

use super::connect;
use crate::remotes::get_remote;

pub const ROUTER: Router = Router::new()
    //.get(&API_METHOD_LIST_TASKS)
    .match_all("upid", &UPID_API_ROUTER);

pub const UPID_API_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(UPID_API_SUBDIRS))
    //.delete(&API_METHOD_STOP_TASK)
    .subdirs(UPID_API_SUBDIRS);

#[sortable]
const UPID_API_SUBDIRS: SubdirMap = &sorted!([
    ("log", &Router::new().get(&API_METHOD_READ_TASK_LOG)),
    ("status", &Router::new().get(&API_METHOD_GET_TASK_STATUS)),
]);

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            upid: { type: RemoteUpid },
            wait: {
                description: "wait for the task to finish before returning its result",
                type: Boolean,
                optional: true,
                default: false,
            },
        },
    },
    returns: { type: pve_client::types::TaskStatus },
)]
/// Get the status of a task from a Proxmox VE instance.
async fn get_task_status(
    remote: String,
    upid: RemoteUpid,
    wait: bool,
) -> Result<pve_client::types::TaskStatus, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    if upid.remote() != remote {
        bail!(
            "remote '{remote}' does not match remote in upid ('{}')",
            upid.remote()
        );
    }

    let pve = match get_remote(&remotes, upid.remote())? {
        Remote::Pve(pve) => pve,
    };

    let pve_upid: PveUpid = upid
        .upid
        .parse()
        .map_err(|err| format_err!("invalid upid for PVE: {} - {err}", upid.upid))?;

    let pve = connect(pve)?;

    loop {
        let status = pve.get_task_status(&pve_upid.node, &upid.upid).await?;
        if !wait || !status.is_running() {
            break Ok(status);
        }
    }
}

// FIXME: Deduplicate these into pdm_api_types:
const START_PARAM_SCHEMA: Schema =
    proxmox_schema::IntegerSchema::new("Start at this line when reading the tasklog")
        .minimum(0)
        .default(0)
        .schema();

const LIMIT_PARAM_SCHEMA: Schema = proxmox_schema::IntegerSchema::new(
    "The amount of lines to read from the tasklog. \
         Setting this parameter to 0 will return all lines until the end of the file.",
)
.minimum(0)
.default(50)
.schema();

const DOWNLOAD_PARAM_SCHEMA: Schema = proxmox_schema::BooleanSchema::new(
    "Whether the tasklog file should be downloaded. \
        This parameter can't be used in conjunction with other parameters",
)
.default(false)
.schema();

// FIXME: make *actually* streaming with router support!
#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            upid: { type: RemoteUpid },
            start: {
                schema: START_PARAM_SCHEMA,
                optional: true,
            },
            limit: {
                schema: LIMIT_PARAM_SCHEMA,
                optional: true,
            },
            download: {
                schema: DOWNLOAD_PARAM_SCHEMA,
                optional: true,
            }
        },
    },
    returns: { type: pve_client::types::TaskStatus },
)]
/// Read a task log.
async fn read_task_log(
    remote: String,
    upid: RemoteUpid,
    download: Option<bool>,
    start: Option<u64>,
    limit: Option<u64>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<pve_client::types::TaskLogLine>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    if upid.remote() != remote {
        bail!(
            "remote '{remote}' does not match remote in upid ('{}')",
            upid.remote()
        );
    }

    let pve = match get_remote(&remotes, upid.remote())? {
        Remote::Pve(pve) => pve,
    };

    let pve_upid: PveUpid = upid
        .upid
        .parse()
        .map_err(|err| format_err!("invalid upid for PVE: {} - {err}", upid.upid))?;

    let pve = connect(pve)?;

    let response = pve
        .get_task_log(&pve_upid.node, &upid.upid, download, limit, start)
        .await?;

    for (key, value) in response.attribs {
        rpcenv[&key] = value;
    }

    Ok(response.data.unwrap_or_default())
}
