use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Error;
use nix::sys::stat::Mode;
use tokio::{sync::Semaphore, task::JoinSet};

use pdm_api_types::{
    remotes::{Remote, RemoteType},
    RemoteUpid,
};
use proxmox_section_config::typed::SectionConfigData;

use server::{
    connection,
    parallel_fetcher::{NodeResults, ParallelFetcher},
    pbs_client,
    remote_tasks::{
        self,
        task_cache::{GetTasks, NodeFetchSuccessMap, State, TaskCache, TaskCacheItem},
        KEEP_OLD_FILES, REMOTE_TASKS_DIR, ROTATE_AFTER,
    },
    task_utils,
};

/// Tick interval for the remote task fetching task.
/// This is also the rate at which we check on tracked tasks.
const POLL_INTERVAL: Duration = Duration::from_secs(10);

/// Interval in seconds at which to fetch the newest tasks from remotes (if there is no tracked
/// task for this remote).
const TASK_FETCH_INTERVAL: Duration = Duration::from_secs(600);

/// Interval in seconds at which we poll active tasks. This only really affects 'foreign' (as in,
/// not started by PDM) tasks. Tasks which were started by PDM are always 'tracked' and therefore
/// polled at the interval set in [`POLL_INTERVAL`].
// NOTE: Since we at the moment never query active tasks from remotes, this is merely a safeguard
// to clear stuck active tasks from a previous bug. If we at some point query active tasks, we
// might lower this interval.
const POLL_ACTIVE_INTERVAL: Duration = Duration::from_secs(600);

/// Interval at which to check for task cache rotation.
const CHECK_ROTATE_INTERVAL: Duration = Duration::from_secs(3600);

/// Interval at which the task cache journal should be applied.
///
/// Choosing a value here is a trade-off between performance and avoiding unnecessary writes.
/// Letting the journal grow large avoids writes, but since the journal is not sorted, accessing
/// it will be slower than the task archive itself, as the entire journal must be loaded into
/// memory and then sorted by task starttime. Applying the journal more often might
/// lead to more writes, but should yield better performance.
const APPLY_JOURNAL_INTERVAL: Duration = Duration::from_secs(3600);

/// Maximum number of concurrent connections per remote.
const CONNECTIONS_PER_PVE_REMOTE: usize = 5;

/// Maximum number of total concurrent connections.
const MAX_CONNECTIONS: usize = 20;

/// Maximum number of tasks to fetch from a single remote in one API call.
const MAX_TASKS_TO_FETCH: u64 = 5000;

/// (Ephemeral) Remote task fetching task state.
struct TaskState {
    /// Time at which we last checked for archive rotation.
    last_rotate_check: Instant,
    /// Time at which we fetch tasks the last time.
    last_fetch: Instant,
    /// Time at which we last applied the journal.
    last_journal_apply: Instant,
    /// Time at which we polled active tasks. This is done to ensure that
    /// active tasks are never stuck in the 'active' state
    last_active_poll: Instant,
}

impl TaskState {
    fn new() -> Self {
        let now = Instant::now();

        Self {
            last_rotate_check: now - CHECK_ROTATE_INTERVAL,
            last_fetch: now - TASK_FETCH_INTERVAL,
            last_journal_apply: now - APPLY_JOURNAL_INTERVAL,
            last_active_poll: now - POLL_ACTIVE_INTERVAL,
        }
    }

    /// Reset the task archive rotation timestamp.
    fn reset_rotate_check(&mut self) {
        self.last_rotate_check = Instant::now();
    }

    /// Reset the task fetch timestamp.
    fn reset_fetch(&mut self) {
        self.last_fetch = Instant::now();
    }

    /// Reset the journal apply timestamp.
    fn reset_journal_apply(&mut self) {
        self.last_journal_apply = Instant::now();
    }

    /// Reset the journal apply timestamp.
    fn reset_active_poll(&mut self) {
        self.last_active_poll = Instant::now();
    }

    /// Should we check for archive rotation?
    fn is_due_for_rotate_check(&self) -> bool {
        Instant::now().duration_since(self.last_rotate_check) > CHECK_ROTATE_INTERVAL
    }

    /// Should we fetch tasks?
    fn is_due_for_fetch(&self) -> bool {
        Instant::now().duration_since(self.last_fetch) > TASK_FETCH_INTERVAL
    }

    /// Should we apply the task archive's journal?
    fn is_due_for_journal_apply(&self) -> bool {
        Instant::now().duration_since(self.last_journal_apply) > APPLY_JOURNAL_INTERVAL
    }

    /// Should we poll active tasks?
    fn is_due_for_active_poll(&self) -> bool {
        Instant::now().duration_since(self.last_active_poll) > POLL_ACTIVE_INTERVAL
    }
}

/// Start the remote task fetching task
pub fn start_task() -> Result<(), Error> {
    let dir_options =
        proxmox_product_config::default_create_options().perm(Mode::from_bits_truncate(0o0750));

    proxmox_sys::fs::create_path(REMOTE_TASKS_DIR, None, Some(dir_options))?;

    tokio::spawn(async move {
        let task_scheduler = std::pin::pin!(remote_task_fetching_task());
        let abort_future = std::pin::pin!(proxmox_daemon::shutdown_future());
        futures::future::select(task_scheduler, abort_future).await;
    });

    Ok(())
}

/// Task which handles fetching remote tasks and task archive rotation.
/// This function never returns.
async fn remote_task_fetching_task() -> ! {
    let mut task_state = TaskState::new();

    let mut interval = tokio::time::interval(POLL_INTERVAL);
    interval.reset_at(task_utils::next_aligned_instant(POLL_INTERVAL.as_secs()).into());

    // We don't really care about catching up to missed tick, we just want
    // a steady tick rate.
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    if let Err(err) = init_cache().await {
        log::error!("error when initialized task cache: {err:#}");
    }

    loop {
        interval.tick().await;
        if let Err(err) = do_tick(&mut task_state).await {
            log::error!("error when fetching remote tasks: {err:#}");
        }
    }
}

/// Handle a single timer tick.
/// Will handle archive file rotation, polling of tracked tasks and fetching or remote tasks.
async fn do_tick(task_state: &mut TaskState) -> Result<(), Error> {
    let cache = remote_tasks::get_cache()?;

    if task_state.is_due_for_rotate_check() {
        log::debug!("checking if remote task archive should be rotated");
        if rotate_cache(cache.clone()).await? {
            log::info!("rotated remote task archive");
        }

        task_state.reset_rotate_check();
    }

    if task_state.is_due_for_journal_apply() {
        apply_journal(cache.clone()).await?;
        task_state.reset_journal_apply();
    }

    let (remote_config, _) = tokio::task::spawn_blocking(pdm_config::remotes::config).await??;

    let total_connections_semaphore = Arc::new(Semaphore::new(MAX_CONNECTIONS));

    let cache_state = cache.read_state();

    let poll_results = if task_state.is_due_for_active_poll() {
        let mut tasks_to_poll: HashSet<RemoteUpid> =
            HashSet::from_iter(cache_state.tracked_tasks().cloned());

        let active_tasks = get_active_tasks(cache.clone()).await?;
        tasks_to_poll.extend(active_tasks.into_iter());

        let poll_results = poll_tracked_tasks(
            &remote_config,
            tasks_to_poll.iter(),
            Arc::clone(&total_connections_semaphore),
        )
        .await?;

        task_state.reset_active_poll();

        poll_results
    } else {
        poll_tracked_tasks(
            &remote_config,
            cache_state.tracked_tasks(),
            Arc::clone(&total_connections_semaphore),
        )
        .await?
    };

    // Get a list of remotes that we should poll in this cycle.
    let remotes = if task_state.is_due_for_fetch() {
        task_state.reset_fetch();
        get_all_remotes(&remote_config)
    } else {
        get_remotes_with_finished_tasks(&remote_config, &poll_results)
    };

    let (all_tasks, update_state_for_remote) = fetch_remotes(remotes, Arc::new(cache_state)).await;

    if !all_tasks.is_empty()
        || poll_results
            .iter()
            .any(|(_, result)| matches!(result, PollResult::RemoteGone | PollResult::RequestError))
    {
        update_task_cache(cache, all_tasks, update_state_for_remote, poll_results).await?;
    }

    Ok(())
}

/// Initialize the remote task cache with initial archive files, in case there are not
/// any archive files yet.
///
/// This allows us to immediately backfill remote task history when setting up a new PDM instance
/// without any prior task archive rotation.
async fn init_cache() -> Result<(), Error> {
    tokio::task::spawn_blocking(|| {
        let cache = remote_tasks::get_cache()?;
        cache.write()?.init(proxmox_time::epoch_i64())?;
        Ok(())
    })
    .await?
}

/// Fetch tasks from a list of remotes.
///
/// Returns a list of tasks and a map that shows whether we want to update the
/// cutoff timestamp in the statefile. We don't want to update the cutoff if
/// the connection to one remote failed or if we could not reach all remotes in a cluster.
async fn fetch_remotes(
    remotes: Vec<Remote>,
    cache_state: Arc<State>,
) -> (Vec<TaskCacheItem>, NodeFetchSuccessMap) {
    let fetcher = ParallelFetcher {
        max_connections: MAX_CONNECTIONS,
        max_connections_per_remote: CONNECTIONS_PER_PVE_REMOTE,
        context: cache_state,
    };

    let fetch_results = fetcher
        .do_for_all_remote_nodes(remotes.into_iter(), fetch_tasks_from_single_node)
        .await;

    let mut all_tasks = Vec::new();
    let mut node_success_map = NodeFetchSuccessMap::default();

    for (remote_name, result) in fetch_results.remote_results {
        match result {
            Ok(remote_result) => {
                for (node_name, node_result) in remote_result.node_results {
                    match node_result {
                        Ok(NodeResults { data, .. }) => {
                            all_tasks.extend(data);
                            node_success_map.set_node_success(remote_name.clone(), node_name);
                        }
                        Err(err) => {
                            log::error!("could not fetch tasks from remote '{remote_name}', node {node_name}: {err:#}");
                        }
                    }
                }
            }
            Err(err) => {
                log::error!("could not fetch tasks from remote '{remote_name}': {err:#}");
            }
        }
    }

    (all_tasks, node_success_map)
}

async fn fetch_tasks_from_single_node(
    context: Arc<State>,
    remote: Remote,
    node: String,
) -> Result<Vec<TaskCacheItem>, Error> {
    let since = context
        .cutoff_timestamp(&remote.id, &node)
        .unwrap_or_else(|| {
            proxmox_time::epoch_i64() - (KEEP_OLD_FILES as u64 * ROTATE_AFTER) as i64
        });

    match remote.ty {
        RemoteType::Pve => {
            let params = pve_api_types::ListTasks {
                source: Some(pve_api_types::ListTasksSource::Archive),
                since: Some(since),
                // If `limit` is not provided, we only receive 50 tasks
                limit: Some(MAX_TASKS_TO_FETCH),
                ..Default::default()
            };

            let client = connection::make_pve_client(&remote)?;

            let task_list = client
                .get_task_list(&node, params)
                .await?
                .into_iter()
                .map(|task| map_pve_task(task, remote.id.clone()))
                .collect();

            Ok(task_list)
        }
        RemoteType::Pbs => {
            let params = pbs_client::ListTasks {
                since: Some(since),
                // If `limit` is not provided, we only receive 50 tasks
                limit: Some(MAX_TASKS_TO_FETCH),
            };

            let client = connection::make_pbs_client(&remote)?;

            let task_list = client
                .get_task_list(params)
                .await?
                .into_iter()
                .filter_map(|task| {
                    if task.endtime.is_some() {
                        // We only care about finished tasks.
                        Some(map_pbs_task(task, remote.id.clone()))
                    } else {
                        None
                    }
                })
                .collect();

            Ok(task_list)
        }
    }
}

/// Return all remotes from the given config.
fn get_all_remotes(remote_config: &SectionConfigData<Remote>) -> Vec<Remote> {
    remote_config
        .into_iter()
        .map(|(_, section)| section)
        .cloned()
        .collect()
}

/// Return all remotes that correspond to a list of finished tasks.
fn get_remotes_with_finished_tasks(
    remote_config: &SectionConfigData<Remote>,
    poll_results: &HashMap<RemoteUpid, PollResult>,
) -> Vec<Remote> {
    let remotes_with_finished_tasks: HashSet<&str> = poll_results
        .iter()
        .filter_map(|(upid, status)| (*status == PollResult::Finished).then_some(upid.remote()))
        .collect();

    remote_config
        .into_iter()
        .filter_map(|(name, remote)| {
            remotes_with_finished_tasks
                .contains(&name)
                .then_some(remote)
        })
        .cloned()
        .collect()
}

/// Rotate the task cache if necessary.
///
/// Returns Ok(true) the cache's files were rotated.
async fn rotate_cache(cache: TaskCache) -> Result<bool, Error> {
    tokio::task::spawn_blocking(move || cache.write()?.rotate(proxmox_time::epoch_i64())).await?
}

/// Apply the task cache journal.
async fn apply_journal(cache: TaskCache) -> Result<(), Error> {
    tokio::task::spawn_blocking(move || cache.write()?.apply_journal()).await?
}

/// Get a list of active tasks.
async fn get_active_tasks(cache: TaskCache) -> Result<Vec<RemoteUpid>, Error> {
    Ok(tokio::task::spawn_blocking(move || {
        let tasks: Vec<RemoteUpid> = cache
            .read()?
            .get_tasks(GetTasks::Active)?
            .map(|t| t.upid)
            .collect();

        Ok::<Vec<RemoteUpid>, Error>(tasks)
    })
    .await??)
}

#[derive(PartialEq, Debug)]
/// Outcome from polling a tracked task.
enum PollResult {
    /// Tasks is still running.
    Running,
    /// Task is finished, poll remote tasks to get final status/endtime.
    Finished,
    /// Should be dropped from the active file.
    RequestError,
    /// Remote does not exist any more -> remove immediately from tracked task list.
    RemoteGone,
}

/// Poll all tracked tasks.
async fn poll_tracked_tasks(
    remote_config: &SectionConfigData<Remote>,
    tracked_tasks: impl Iterator<Item = &RemoteUpid>,
    total_connections_semaphore: Arc<Semaphore>,
) -> Result<HashMap<RemoteUpid, PollResult>, Error> {
    let mut join_set = JoinSet::new();

    for task in tracked_tasks.cloned() {
        let permit = Arc::clone(&total_connections_semaphore)
            .acquire_owned()
            .await
            .unwrap();

        let remote = remote_config.get(task.remote()).cloned();

        join_set.spawn(async move {
            // Move permit into this async block.
            let _permit = permit;

            match remote {
                Some(remote) => poll_single_tracked_task(remote, task).await,
                None => {
                    log::info!(
                        "remote {} does not exist any more, dropping tracked task",
                        task.remote()
                    );
                    (task, PollResult::RemoteGone)
                }
            }
        });
    }

    let mut results = HashMap::new();
    while let Some(task_result) = join_set.join_next().await {
        let (upid, result) = task_result?;
        results.insert(upid, result);
    }

    Ok(results)
}

/// Poll a single tracked task.
async fn poll_single_tracked_task(remote: Remote, task: RemoteUpid) -> (RemoteUpid, PollResult) {
    match remote.ty {
        RemoteType::Pve => {
            log::debug!("polling tracked task {}", task);

            let status = match server::api::pve::tasks::get_task_status(
                remote.id.clone(),
                task.clone(),
                false,
            )
            .await
            {
                Ok(status) => status,
                Err(err) => {
                    log::error!("could not get status from remote: {err:#}");
                    return (task, PollResult::RequestError);
                }
            };

            let result = if status.exitstatus.is_some() {
                PollResult::Finished
            } else {
                PollResult::Running
            };

            (task, result)
        }
        RemoteType::Pbs => {
            let status = match server::api::pbs::tasks::get_task_status(
                remote.id.clone(),
                task.clone(),
                false,
            )
            .await
            {
                Ok(status) => status,
                Err(err) => {
                    log::error!("could not get status from remote: {err:#}");
                    return (task, PollResult::RequestError);
                }
            };

            let result = if status.exitstatus.is_some() {
                PollResult::Finished
            } else {
                PollResult::Running
            };

            (task, result)
        }
    }
}

/// Map a `pve_api_types::ListTasksResponse` to `TaskCacheItem`
fn map_pve_task(task: pve_api_types::ListTasksResponse, remote: String) -> TaskCacheItem {
    let remote_upid = RemoteUpid::new(remote, RemoteType::Pve, task.upid);

    TaskCacheItem {
        upid: remote_upid,
        starttime: task.starttime,
        endtime: task.endtime,
        status: task.status,
    }
}

/// Map a `pbs_api_types::TaskListItem` to `TaskCacheItem`
fn map_pbs_task(task: pbs_api_types::TaskListItem, remote: String) -> TaskCacheItem {
    let remote_upid = RemoteUpid::new(remote, RemoteType::Pbs, task.upid);

    TaskCacheItem {
        upid: remote_upid,
        starttime: task.starttime,
        endtime: task.endtime,
        status: task.status,
    }
}

/// Update task cache with results from tracked task polling & regular task fetching.
async fn update_task_cache(
    cache: TaskCache,
    new_tasks: Vec<TaskCacheItem>,
    update_state_for_remote: NodeFetchSuccessMap,
    poll_results: HashMap<RemoteUpid, PollResult>,
) -> Result<(), Error> {
    tokio::task::spawn_blocking(move || {
        let drop_tracked = poll_results
            .into_iter()
            .filter_map(|(upid, result)| match result {
                PollResult::Running => None,
                PollResult::Finished | PollResult::RequestError | PollResult::RemoteGone => {
                    Some(upid)
                }
            })
            .collect();

        cache
            .write()?
            .update(new_tasks, &update_state_for_remote, drop_tracked)?;

        Ok(())
    })
    .await?
}
