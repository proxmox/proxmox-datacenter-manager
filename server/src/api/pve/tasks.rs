//! Access to PVE tasks.

use anyhow::Error;

use proxmox_router::{list_subdirs_api_method, Permission, Router, RpcEnvironment, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use pdm_api_types::remotes::{RemoteType, REMOTE_ID_SCHEMA};
use pdm_api_types::{
    RemoteUpid, NODE_SCHEMA, PRIV_RESOURCE_AUDIT, PRIV_RESOURCE_MANAGE,
    TASKLOG_DOWNLOAD_PARAM_SCHEMA, TASKLOG_LIMIT_PARAM_SCHEMA, TASKLOG_START_PARAM_SCHEMA,
};

use super::{connect, connect_to_remote, get_remote};

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_TASKS)
    .match_all("upid", &UPID_API_ROUTER);

pub const UPID_API_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(UPID_API_SUBDIRS))
    .delete(&API_METHOD_STOP_TASK)
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
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
        },
    },
    access: {
        // FIXME: fine-grained task filtering?
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
    returns: { type: pve_api_types::TaskStatus },
)]
/// Get the list of tasks either for a specific node, or query all at once.
async fn list_tasks(
    remote: String,
    node: Option<String>,
) -> Result<Vec<pve_api_types::ListTasksResponse>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

    if let Some(node) = node {
        Ok(pve.get_task_list(&node, Default::default()).await?)
    } else {
        let mut entry = Vec::new();
        for node in pve.list_nodes().await? {
            entry.extend(pve.get_task_list(&node.node, Default::default()).await?);
        }
        Ok(entry)
    }
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            upid: { type: RemoteUpid },
        },
    },
    access: {
        // FIXME: fine-grained task filtering?
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Get the status of a task from a Proxmox VE instance.
async fn stop_task(remote: String, upid: RemoteUpid) -> Result<(), Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    crate::api::verify_upid(&remote, RemoteType::Pve, &upid)?;

    let pve_upid = upid.pve_upid()?;

    let remote = get_remote(&remotes, upid.remote())?;
    let client = connect(remote)?;

    Ok(client.stop_task(&pve_upid.node, upid.upid()).await?)
}

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
    access: {
        // FIXME: fine-grained task filtering?
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
    returns: { type: pve_api_types::TaskStatus },
)]
/// Get the status of a task from a Proxmox VE instance.
pub async fn get_task_status(
    remote: String,
    upid: RemoteUpid,
    wait: bool,
) -> Result<pve_api_types::TaskStatus, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    crate::api::verify_upid(&remote, RemoteType::Pve, &upid)?;

    let pve_upid = upid.pve_upid()?;

    let remote = get_remote(&remotes, upid.remote())?;
    let client = connect(remote)?;

    loop {
        let status = client.get_task_status(&pve_upid.node, upid.upid()).await?;
        if !wait || !status.is_running() {
            break Ok(status);
        }
    }
}

// FIXME: make *actually* streaming with router support!
#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            upid: { type: RemoteUpid },
            start: {
                schema: TASKLOG_START_PARAM_SCHEMA,
                optional: true,
            },
            limit: {
                schema: TASKLOG_LIMIT_PARAM_SCHEMA,
                optional: true,
            },
            download: {
                schema: TASKLOG_DOWNLOAD_PARAM_SCHEMA,
                optional: true,
            }
        },
    },
    access: {
        // FIXME: fine-grained task filtering?
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
    returns: { type: pve_api_types::TaskStatus },
)]
/// Read a task log.
async fn read_task_log(
    remote: String,
    upid: RemoteUpid,
    download: Option<bool>,
    start: Option<u64>,
    limit: Option<u64>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<pve_api_types::TaskLogLine>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    crate::api::verify_upid(&remote, RemoteType::Pve, &upid)?;

    let pve_upid = upid.pve_upid()?;

    let remote = get_remote(&remotes, upid.remote())?;
    let client = connect(remote)?;

    let response = client
        .get_task_log(&pve_upid.node, upid.upid(), download, limit, start)
        .await?;

    for (key, value) in response.attribs {
        rpcenv[&key] = value;
    }

    Ok(response.data)
}
