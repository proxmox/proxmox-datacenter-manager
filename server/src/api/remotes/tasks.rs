use std::collections::HashMap;

use anyhow::{Context, Error};

use pdm_api_types::{
    remotes::REMOTE_ID_SCHEMA, Authid, RemoteUpid, TaskCount, TaskFilters, TaskListItem,
    TaskStateType, TaskStatistics, PRIV_RESOURCE_AUDIT, PRIV_RESOURCE_MODIFY, UPID, VIEW_ID_SCHEMA,
};
use proxmox_access_control::CachedUserInfo;
use proxmox_rest_server::WorkerTask;
use proxmox_router::{
    http_bail, http_err, list_subdirs_api_method, Permission, Router, RpcEnvironment, SubdirMap,
};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use crate::remote_tasks;

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("list", &Router::new().get(&API_METHOD_LIST_TASKS)),
    (
        "statistics",
        &Router::new().get(&API_METHOD_TASK_STATISTICS)
    ),
    (
        "refresh",
        &Router::new().post(&API_METHOD_REFRESH_REMOTE_TASKS)
    )
]);

#[api(
    access: {
        permission: &Permission::Anybody,
        description: "Resource.Audit privileges on /resource/{remote} are needed to list tasks from a given remote."
    },
    input: {
        properties: {
            filters: {
                type: TaskFilters,
                flatten: true,
            },
            remote: {
                schema: REMOTE_ID_SCHEMA,
                optional: true,
            },
            view: {
                schema: VIEW_ID_SCHEMA,
                optional: true,
            },

        },
    },
    returns: {
        type: Array,
        description: "A list of tasks for all remotes.",
        items: {
            type: TaskListItem
        }
    },
)]
/// Get the list of tasks for all remotes.
async fn list_tasks(
    filters: TaskFilters,
    remote: Option<String>,
    view: Option<String>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<TaskListItem>, Error> {
    let auth_id = rpcenv
        .get_auth_id()
        .context("no authid available")?
        .parse()?;
    let user_info = CachedUserInfo::new()?;

    if let Some(view) = &view {
        user_info.check_privs(&auth_id, &["view", view], PRIV_RESOURCE_AUDIT, false)?;
    }

    let check_privs = move |remote_name: &str| {
        user_info
            .check_privs(
                &auth_id,
                &["resource", remote_name],
                PRIV_RESOURCE_AUDIT,
                false,
            )
            .is_ok()
    };

    let tasks = remote_tasks::get_tasks(filters, remote, check_privs, view).await?;

    Ok(tasks)
}

#[api(
    access: {
        permission: &Permission::Anybody,
        description: "Resource.Audit privileges on /resource/{remote} are needed to list tasks from a given remote."
    },
    input: {
        properties: {
            filters: {
                type: TaskFilters,
                flatten: true,
            },
            remote: {
                schema: REMOTE_ID_SCHEMA,
                optional: true,
            },
            view: {
                schema: VIEW_ID_SCHEMA,
                optional: true,
            },
        },
    },
    returns: { type: TaskStatistics }
)]
/// Get task statistics for the specified filters.
async fn task_statistics(
    filters: TaskFilters,
    remote: Option<String>,
    view: Option<String>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<TaskStatistics, Error> {
    let auth_id = rpcenv
        .get_auth_id()
        .context("no authid available")?
        .parse()?;
    let user_info = CachedUserInfo::new()?;

    if let Some(view) = &view {
        user_info.check_privs(&auth_id, &["view", view], PRIV_RESOURCE_AUDIT, false)?;
    }

    let check_privs = move |remote_name: &str| {
        user_info
            .check_privs(
                &auth_id,
                &["resource", remote_name],
                PRIV_RESOURCE_AUDIT,
                false,
            )
            .is_ok()
    };

    let tasks = remote_tasks::get_tasks(filters, remote, check_privs, view).await?;

    let mut by_type: HashMap<String, TaskCount> = HashMap::new();
    let mut by_remote: HashMap<String, TaskCount> = HashMap::new();

    for task in tasks {
        let status: TaskStateType = match task.status.as_deref() {
            Some(status) => TaskStateType::new_from_str(status),
            None => continue,
        };
        let entry = by_type.entry(task.worker_type).or_default();
        match status {
            TaskStateType::OK => entry.ok += 1,
            TaskStateType::Warning => entry.warning += 1,
            TaskStateType::Error => entry.error += 1,
            TaskStateType::Unknown => entry.unknown += 1,
        }

        let remote = match task.upid.parse::<RemoteUpid>() {
            Ok(upid) => upid.remote().to_owned(),
            Err(_) => continue,
        };

        let entry = by_remote.entry(remote).or_default();
        match status {
            TaskStateType::OK => entry.ok += 1,
            TaskStateType::Warning => entry.warning += 1,
            TaskStateType::Error => entry.error += 1,
            TaskStateType::Unknown => entry.unknown += 1,
        }
    }

    Ok(TaskStatistics { by_type, by_remote })
}

#[api(
    input: {
        properties: {
            remotes: {
                description: "Optional list of tasks to fetch tasks from.",
                optional: true,
                type: Array,
                items: {
                    schema: REMOTE_ID_SCHEMA,
                },
            },
        },
    },
    access: {
        permission: &Permission::Anybody,
        description: "Resource.Modify privileges are needed on /resource/{remote}",
    },
    returns: { type: UPID }
)]
/// Refresh remote tasks cache.
///
/// If `remotes` is provided, tasks will only be fetched from the provided remotes.
/// `Resource.Modify` permissions on `/resource/{remote}` are needed for every remote provided this way.
///
/// If `remotes` is not provided, then all remotes for which the user has appropriate permissions are considered.
pub fn refresh_remote_tasks(
    remotes: Option<Vec<String>>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<UPID, Error> {
    let (config, _digest) = pdm_config::remotes::config()?;

    let auth_id: Authid = rpcenv.get_auth_id().unwrap().parse()?;
    let user_info = CachedUserInfo::new()?;

    let remotes = if let Some(remotes) = remotes {
        let mut configs = Vec::new();

        for remote in &remotes {
            if user_info
                .check_privs(&auth_id, &["resource", remote], PRIV_RESOURCE_MODIFY, false)
                .is_err()
            {
                http_bail!(FORBIDDEN, "user has no access to this remote");
            }
            let remote = config
                .get(remote)
                .ok_or_else(|| http_err!(NOT_FOUND, "remote does not exist"))?;

            configs.push(remote.clone());
        }

        configs
    } else {
        if !user_info.any_privs_below(&auth_id, &["resource"], PRIV_RESOURCE_MODIFY)? {
            http_bail!(FORBIDDEN, "user has no access to resources");
        }

        config
            .into_iter()
            .filter_map(|(remote_name, remote)| {
                user_info
                    .check_privs(
                        &auth_id,
                        &["resource", &remote_name],
                        PRIV_RESOURCE_MODIFY,
                        false,
                    )
                    .is_ok()
                    .then_some(remote)
            })
            .collect()
    };

    let upid_str = WorkerTask::spawn(
        "refresh-remote-tasks",
        None,
        auth_id.to_string(),
        true,
        |_worker| async {
            remote_tasks::refresh_task::refresh_taskcache(remotes).await?;
            Ok(())
        },
    )?;

    upid_str.parse()
}
