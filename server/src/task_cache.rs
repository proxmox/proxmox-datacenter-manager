use std::{
    collections::{HashMap, HashSet},
    fs::File,
    path::{Path, PathBuf},
    sync::{LazyLock, RwLock},
    time::Duration,
};

use anyhow::Error;
use pdm_api_types::{
    remotes::{Remote, RemoteType},
    RemoteUpid, TaskFilters, TaskListItem, TaskStateType,
};
use proxmox_sys::fs::CreateOptions;
use pve_api_types::{ListTasks, ListTasksResponse, ListTasksSource, PveUpid};
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;

use crate::{api::pve, task_utils};

/// Get tasks for all remotes
// FIXME: filter for privileges
pub async fn get_tasks(max_age: i64, filters: TaskFilters) -> Result<Vec<TaskListItem>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let mut all_tasks = Vec::new();

    let cache_path = Path::new(pdm_buildcfg::PDM_CACHE_DIR).join("taskcache.json");
    let mut cache = TaskCache::new(cache_path)?;

    // Force a refresh for all tasks of a remote if a task is finished.
    // Not super nice, but saves us from persisting finished tasks. Also,
    // the /nodes/<node>/tasks/<upid>/status endpoint does not return
    // a task's endtime, which is only returned by
    // /nodes/<node>/tasks...
    // Room for improvements in the future.
    invalidate_cache_for_finished_tasks(&mut cache);

    for (remote_name, remote) in &remotes.sections {
        let now = proxmox_time::epoch_i64();

        if let Some(tasks) = cache.get_tasks(remote_name.as_str(), now, max_age) {
            // Data in cache is recent enough and has not been invalidated.
            all_tasks.extend(tasks);
        } else {
            let tasks = match fetch_tasks(remote).await {
                Ok(tasks) => tasks,
                Err(err) => {
                    // ignore errors for not reachable remotes
                    continue;
                }
            };
            cache.set_tasks(remote_name.as_str(), tasks.clone(), now);

            all_tasks.extend(tasks);
        }
    }

    let mut returned_tasks = add_running_tasks(all_tasks)?;
    returned_tasks.sort_by(|a, b| a.starttime.cmp(&b.starttime));
    let returned_tasks = returned_tasks
        .into_iter()
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
        .skip(filters.start as usize)
        .take(filters.limit as usize)
        .collect();

    // We don't need to wait for this task to finish
    tokio::task::spawn_blocking(move || {
        if let Err(e) = cache.save() {
            log::error!("could not save task cache: {e}");
        }
    });

    Ok(returned_tasks)
}

/// Fetch tasks (active and finished) from a remote
async fn fetch_tasks(remote: &Remote) -> Result<Vec<TaskListItem>, Error> {
    let mut tasks = Vec::new();

    match remote.ty {
        RemoteType::Pve => {
            let client = pve::connect(remote)?;

            // N+1 requests - we could use /cluster/tasks, but that one
            // only gives a limited task history
            for node in client.list_nodes().await? {
                let params = ListTasks {
                    // Include running tasks
                    source: Some(ListTasksSource::All),
                    // TODO: How much task history do we want? Right now we just hard-code it
                    // to 7 days.
                    since: Some(proxmox_time::epoch_i64() - 7 * 24 * 60 * 60),
                    ..Default::default()
                };

                let list = client.get_task_list(&node.node, params).await?;
                let mapped = map_tasks(list, &remote.id)?;

                tasks.extend(mapped);
            }
        }
        RemoteType::Pbs => {
            // TODO: Add code for PBS
        }
    }

    Ok(tasks)
}

/// Convert a `Vec<ListTaskResponce>` to `Vec<TaskListItem>`
fn map_tasks(tasks: Vec<ListTasksResponse>, remote: &str) -> Result<Vec<TaskListItem>, Error> {
    let mut mapped = Vec::new();

    for task in tasks {
        let remote_upid: RemoteUpid = (remote.to_string(), task.upid.to_string()).try_into()?;

        mapped.push(TaskListItem {
            upid: remote_upid.to_string(),
            node: task.node,
            pid: task.pid,
            pstart: task.pstart as u64,
            starttime: task.starttime,
            worker_type: task.ty,
            worker_id: Some(task.id),
            user: task.user,
            endtime: task.endtime,
            status: task.status,
        })
    }

    Ok(mapped)
}

/// Drops the cached task list of a remote for all finished tasks.
///
/// We use this to force a refresh so that we get the full task
/// info (including `endtime`) in the next API call.
fn invalidate_cache_for_finished_tasks(cache: &mut TaskCache) {
    let mut finished = FINISHED_FOREIGN_TASKS.write().expect("mutex poisoned");

    // If a task is finished, we force a refresh for the remote - otherwise
    // we don't get the 'endtime' for the task.
    for task in finished.drain() {
        cache.invalidate_cache_for_remote(task.remote());
    }
}

/// Supplement the list of tasks that we received from the remote with
/// the tasks that were started by PDM and are currently running.
fn add_running_tasks(cached_tasks: Vec<TaskListItem>) -> Result<Vec<TaskListItem>, Error> {
    let mut returned_tasks = Vec::new();

    let mut running_tasks = RUNNING_FOREIGN_TASKS.write().expect("mutex poisoned");
    for task in cached_tasks {
        let remote_upid = task.upid.parse()?;

        if running_tasks.contains(&remote_upid) {
            if task.endtime.is_some() {
                // Task is finished but we still think it is running ->
                // Drop it from RUNNING_FOREIGN_TASKS
                running_tasks.remove(&remote_upid);

                // No need to put it in FINISHED_TASKS, since we already
                // got its state recently enough (we know the status and endtime)
            }
        } else {
            returned_tasks.push(task);
        }
    }

    for task in running_tasks.iter() {
        let upid: PveUpid = task.upid.parse()?;
        returned_tasks.push(TaskListItem {
            upid: task.to_string(),
            node: upid.node,
            pid: upid.pid as i64,
            pstart: upid.pstart,
            starttime: upid.starttime,
            worker_type: upid.worker_type,
            worker_id: upid.worker_id,
            user: upid.auth_id,
            endtime: None,
            status: None,
        });
    }

    Ok(returned_tasks)
}

/// A cache for fetched remote tasks.
struct TaskCache {
    /// Cache entries
    content: TaskCacheContent,

    /// Entries that were added or updated - these will be persistet
    /// when `save` is called.
    new_or_updated: TaskCacheContent,

    /// Cache entries were changed/removed.
    dirty: bool,

    /// File-location at which the cached tasks are stored.
    cachefile_path: PathBuf,
}

impl TaskCache {
    /// Create a new tasks cache instance by loading
    /// the cache from disk.
    fn new(cachefile_path: PathBuf) -> Result<Self, Error> {
        Ok(Self {
            content: Self::load_content()?,
            new_or_updated: Default::default(),
            dirty: false,
            cachefile_path,
        })
    }

    /// Load the task cache contents from disk.
    fn load_content() -> Result<TaskCacheContent, Error> {
        let taskcache_path = Path::new(pdm_buildcfg::PDM_CACHE_DIR).join("taskcache.json");
        let content = proxmox_sys::fs::file_read_optional_string(taskcache_path)?;

        let content = if let Some(content) = content {
            serde_json::from_str(&content)?
        } else {
            Default::default()
        };

        Ok(content)
    }

    /// Get path for the cache's lockfile.
    fn lockfile_path(&self) -> PathBuf {
        let mut path = self.cachefile_path.clone();
        path.set_extension("lock");
        path
    }

    /// Persist the task cache
    ///
    /// This method requests an exclusive lock for the task cache lockfile.
    fn save(&mut self) -> Result<(), Error> {
        // if we have not updated anything, we don't have to update the cache file
        if !self.dirty {
            return Ok(());
        }

        let _guard = self.lock(Duration::from_secs(5))?;

        // Read content again, in case somebody has changed it in the meanwhile
        let mut content = Self::load_content()?;

        for (remote_name, entry) in self.new_or_updated.remote_tasks.drain() {
            if let Some(existing_entry) = content.remote_tasks.get_mut(&remote_name) {
                // Only update entry if nobody else has updated it in the meanwhile
                if existing_entry.timestamp < entry.timestamp {
                    *existing_entry = entry;
                }
            } else {
                content.remote_tasks.insert(remote_name, entry);
            }
        }

        let bytes = serde_json::to_vec_pretty(&content)?;

        let api_uid = pdm_config::api_user()?.uid;
        let api_gid = pdm_config::api_group()?.gid;

        let file_options = CreateOptions::new().owner(api_uid).group(api_gid);

        proxmox_sys::fs::replace_file(&self.cachefile_path, &bytes, file_options, true)?;

        self.dirty = false;

        Ok(())
    }

    // Update task data for a given remote.
    fn set_tasks(&mut self, remote: &str, tasks: Vec<TaskListItem>, timestamp: i64) {
        self.dirty = true;
        self.new_or_updated
            .remote_tasks
            .insert(remote.to_string(), TaskCacheEntry { timestamp, tasks });
    }

    // Get task data for a given remote.
    fn get_tasks(&self, remote: &str, now: i64, max_age: i64) -> Option<Vec<TaskListItem>> {
        if let Some(entry) = self.content.remote_tasks.get(remote) {
            if (entry.timestamp + max_age) < now {
                return None;
            }

            Some(entry.tasks.clone())
        } else if let Some(entry) = self.new_or_updated.remote_tasks.get(remote) {
            if (entry.timestamp + max_age) < now {
                return None;
            }
            Some(entry.tasks.clone())
        } else {
            None
        }
    }

    // Invalidate cache for a given remote.
    fn invalidate_cache_for_remote(&mut self, remote: &str) {
        self.dirty = true;
        self.content.remote_tasks.remove(remote);
    }

    // Lock the cache for modification.
    //
    // While the cache is locked, other users can still read the cache
    // without a lock, since the cache file is replaced atomically
    // when updating.
    fn lock(&self, duration: Duration) -> Result<File, Error> {
        let api_uid = pdm_config::api_user()?.uid;
        let api_gid = pdm_config::api_group()?.gid;

        let file_options = CreateOptions::new().owner(api_uid).group(api_gid);
        proxmox_sys::fs::open_file_locked(self.lockfile_path(), duration, true, file_options)
    }
}

#[derive(Serialize, Deserialize)]
/// Per-remote entry in the task cache.
struct TaskCacheEntry {
    timestamp: i64,
    tasks: Vec<TaskListItem>,
}

#[derive(Default, Serialize, Deserialize)]
/// Content of the task cache file.
struct TaskCacheContent {
    remote_tasks: HashMap<String, TaskCacheEntry>,
}

/// Interval at which tracked tasks are polled
const RUNNING_CHECK_INTERVAL_S: u64 = 10;

/// Tasks which were started by PDM and are still running
static RUNNING_FOREIGN_TASKS: LazyLock<RwLock<HashSet<RemoteUpid>>> = LazyLock::new(init);
/// Tasks which were started by PDM and w
static FINISHED_FOREIGN_TASKS: LazyLock<RwLock<HashSet<RemoteUpid>>> = LazyLock::new(init);

fn init() -> RwLock<HashSet<RemoteUpid>> {
    RwLock::new(HashSet::new())
}

/// Insert a remote UPID into the running list
///
/// If it is the first entry in the list, a background task is started to track its state
///
/// Returns the [`JoinHandle`] if a task was started.
///
/// panics on a poisoned mutex
pub fn track_running_task(task: RemoteUpid) -> Option<JoinHandle<()>> {
    let mut tasks = RUNNING_FOREIGN_TASKS.write().unwrap();

    // the call inserting the first task in the list needs to start the checking task
    let need_start_task = tasks.is_empty();
    tasks.insert(task);

    if !need_start_task {
        return None;
    }
    drop(tasks);

    Some(tokio::spawn(async move {
        loop {
            let delay_target = task_utils::next_aligned_instant(RUNNING_CHECK_INTERVAL_S);
            tokio::time::sleep_until(tokio::time::Instant::from_std(delay_target)).await;

            let finished_tasks = get_finished_tasks().await;

            // skip iteration if we still have tasks, just not finished ones
            if finished_tasks.is_empty() && !RUNNING_FOREIGN_TASKS.read().unwrap().is_empty() {
                continue;
            }

            let mut finished = FINISHED_FOREIGN_TASKS.write().unwrap();
            // we either have finished tasks, or the running task list was empty
            let mut set = RUNNING_FOREIGN_TASKS.write().unwrap();

            for (upid, _status) in finished_tasks {
                if set.remove(&upid) {
                    finished.insert(upid);
                } else {
                    // someone else removed & persisted the task in the meantime
                }
            }

            // if no task remains, end the current task
            // it will be restarted by the next caller that inserts one
            if set.is_empty() {
                return;
            }
        }
    }))
}

/// Get a list of running foreign tasks
///
/// panics on a poisoned mutex
pub fn get_running_tasks() -> Vec<RemoteUpid> {
    RUNNING_FOREIGN_TASKS
        .read()
        .unwrap()
        .iter()
        .cloned()
        .collect()
}

/// Checks all current saved UPIDs if they're still running, and if not,
/// returns their upids + status
///
/// panics on a poisoned mutex
pub async fn get_finished_tasks() -> Vec<(RemoteUpid, String)> {
    let mut finished = Vec::new();
    let config = match pdm_config::remotes::config() {
        Ok((config, _)) => config,
        Err(err) => {
            log::error!("could not open remotes config: {err}");
            return Vec::new();
        }
    };
    for task in get_running_tasks() {
        match config.get(task.remote()) {
            Some(remote) => match remote.ty {
                RemoteType::Pve => {
                    let status = match crate::api::pve::tasks::get_task_status(
                        remote.id.clone(),
                        task.clone(),
                        false,
                    )
                    .await
                    {
                        Ok(status) => status,
                        Err(err) => {
                            log::error!("could not get status from remote: {err}");
                            finished.push((task, "could not get status".to_string()));
                            continue;
                        }
                    };
                    if let Some(status) = status.exitstatus {
                        finished.push((task, status.to_string()));
                    }
                }
                RemoteType::Pbs => {
                    let _client = match crate::pbs_client::connect(remote) {
                        Ok(client) => client,
                        Err(err) => {
                            log::error!("could not get status from remote: {err}");
                            finished.push((task, "could not get status".to_string()));
                            continue;
                        }
                    };
                    // FIXME implement get task status
                    finished.push((task, "unknown state".to_string()));
                }
            },
            None => finished.push((task, "unknown remote".to_string())),
        }
    }

    finished
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
