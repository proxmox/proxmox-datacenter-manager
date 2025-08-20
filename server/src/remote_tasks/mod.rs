use std::path::Path;

use anyhow::Error;

use pdm_api_types::{RemoteUpid, TaskFilters, TaskListItem, TaskStateType};
use pve_api_types::PveUpid;

pub mod task_cache;

use task_cache::{GetTasks, TaskCache, TaskCacheItem};

/// Base directory for the remote task cache.
pub const REMOTE_TASKS_DIR: &str = concat!(pdm_buildcfg::PDM_CACHE_DIR_M!(), "/remote-tasks");

/// Maximum size at which the journal will applied early when adding new tasks.
const JOURNAL_MAX_SIZE: u64 = 5 * 1024 * 1024;

/// Rotate once the most recent archive file is at least 24 hour old.
pub const ROTATE_AFTER: u64 = 24 * 3600;

/// Keep 7 days worth of tasks.
pub const KEEP_OLD_FILES: u32 = 7;

/// Number of uncompressed archive files. These will be be the most recent ones.
const NUMBER_OF_UNCOMPRESSED_FILES: u32 = 2;

/// Get tasks for all remotes
// FIXME: filter for privileges
pub async fn get_tasks(filters: TaskFilters) -> Result<Vec<TaskListItem>, Error> {
    tokio::task::spawn_blocking(move || {
        let cache = get_cache()?.read()?;

        let which = if filters.running {
            GetTasks::Active
        } else {
            GetTasks::All
        };

        let returned_tasks = cache
            .get_tasks(which)?
            .skip(filters.start as usize)
            .take(filters.limit as usize)
            .filter_map(|task| {
                // TODO: Handle PBS tasks
                let pve_upid: Result<PveUpid, Error> = task.upid.upid.parse();
                match pve_upid {
                    Ok(pve_upid) => Some(TaskListItem {
                        upid: task.upid.to_string(),
                        node: pve_upid.node,
                        pid: pve_upid.pid as i64,
                        pstart: pve_upid.pstart,
                        starttime: pve_upid.starttime,
                        worker_type: pve_upid.worker_type,
                        worker_id: None,
                        user: pve_upid.auth_id,
                        endtime: task.endtime,
                        status: task.status,
                    }),
                    Err(err) => {
                        log::error!("could not parse UPID: {err:#}");
                        None
                    }
                }
            })
            .filter(|item| {
                if filters.running && item.endtime.is_some() {
                    return false;
                }

                if let Some(until) = filters.until {
                    if item.starttime > until {
                        return false;
                    }
                }

                if let Some(since) = filters.since {
                    if item.starttime < since {
                        return false;
                    }
                }

                if let Some(needle) = &filters.userfilter {
                    if !item.user.contains(needle) {
                        return false;
                    }
                }

                if let Some(typefilter) = &filters.typefilter {
                    if !item.worker_type.contains(typefilter) {
                        return false;
                    }
                }

                let state = item.status.as_ref().map(|status| tasktype(status));

                match (state, &filters.statusfilter) {
                    (Some(TaskStateType::OK), _) if filters.errors => return false,
                    (Some(state), Some(filters)) => {
                        if !filters.contains(&state) {
                            return false;
                        }
                    }
                    (None, Some(_)) => return false,
                    _ => {}
                }

                true
            })
            .collect();

        Ok(returned_tasks)
    })
    .await?
}

/// Insert a newly created tasks into the list of tracked tasks.
///
/// Any tracked task will be polled with a short interval until the task
/// has finished.
pub async fn track_running_task(task: RemoteUpid) -> Result<(), Error> {
    tokio::task::spawn_blocking(move || {
        let cache = get_cache()?.write()?;
        // TODO:: Handle PBS tasks correctly.
        let pve_upid: pve_api_types::PveUpid = task.upid.parse()?;
        let task = TaskCacheItem {
            upid: task.clone(),
            starttime: pve_upid.starttime,
            status: None,
            endtime: None,
        };
        cache.add_tracked_task(task)
    })
    .await?
}

/// Get a new [`TaskCache`] instance.
///
/// No heavy-weight operations are done here, it's fine to call this regularly as part of the
/// update loop.
pub fn get_cache() -> Result<TaskCache, Error> {
    let file_options = proxmox_product_config::default_create_options();

    let cache_path = Path::new(REMOTE_TASKS_DIR);
    let cache = TaskCache::new(
        cache_path,
        file_options,
        KEEP_OLD_FILES,
        NUMBER_OF_UNCOMPRESSED_FILES,
        ROTATE_AFTER,
        JOURNAL_MAX_SIZE,
    )?;

    Ok(cache)
}

/// Parses a task status string into a TaskStateType
pub fn tasktype(status: &str) -> TaskStateType {
    if status == "unknown" || status.is_empty() {
        TaskStateType::Unknown
    } else if status == "OK" {
        TaskStateType::OK
    } else if status.starts_with("WARNINGS: ") {
        TaskStateType::Warning
    } else {
        TaskStateType::Error
    }
}
