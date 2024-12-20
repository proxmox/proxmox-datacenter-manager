use anyhow::Error;
use pdm_api_types::TaskListItem;
use proxmox_router::{list_subdirs_api_method, Permission, Router, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use crate::task_cache;

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
            "max-age": {
                type: Integer,
                optional: true,
                // TODO: sensible default max-age
                default: 300,
                description: "Maximum age of cached task data",
            }
        },
    },
)]
/// Get the list of tasks for all remotes.
async fn list_tasks(max_age: i64) -> Result<Vec<TaskListItem>, Error> {
    let tasks = task_cache::get_tasks(max_age).await?;

    Ok(tasks)
}
