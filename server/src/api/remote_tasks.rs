use std::collections::HashMap;

use anyhow::Error;

use pdm_api_types::{
    remotes::REMOTE_ID_SCHEMA, RemoteUpid, TaskCount, TaskFilters, TaskListItem, TaskStateType,
    TaskStatistics,
};
use proxmox_router::{list_subdirs_api_method, Permission, Router, SubdirMap};
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
]);

#[api(
    // FIXME:: see list-like API calls in resource routers, we probably want more fine-grained
    // checks..
    access: {
        permission: &Permission::Anybody,
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
        },
    },
)]
/// Get the list of tasks for all remotes.
async fn list_tasks(
    filters: TaskFilters,
    remote: Option<String>,
) -> Result<Vec<TaskListItem>, Error> {
    let tasks = remote_tasks::get_tasks(filters, remote).await?;

    Ok(tasks)
}

#[api(
    // FIXME:: see list-like API calls in resource routers, we probably want more fine-grained
    // checks..
    access: {
        permission: &Permission::Anybody,
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
        },
    },
)]
/// Get task statistics for the specified filters.
async fn task_statistics(
    filters: TaskFilters,
    remote: Option<String>,
) -> Result<TaskStatistics, Error> {
    let tasks = remote_tasks::get_tasks(filters, remote).await?;

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
