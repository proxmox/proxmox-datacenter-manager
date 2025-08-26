use anyhow::Error;

use pdm_api_types::{remotes::REMOTE_ID_SCHEMA, TaskFilters, TaskListItem};
use proxmox_router::{list_subdirs_api_method, Permission, Router, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use crate::remote_tasks;

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([("list", &Router::new().get(&API_METHOD_LIST_TASKS)),]);

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
