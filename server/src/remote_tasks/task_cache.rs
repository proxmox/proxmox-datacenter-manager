//! Task cache implementation, based on rotating files.
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, BufWriter, ErrorKind, Lines, Write},
    iter::Peekable,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context, Error};
use serde::{Deserialize, Serialize};

use proxmox_sys::fs::CreateOptions;

use pdm_api_types::RemoteUpid;
use pve_api_types::PveUpid;

/// Filename for the file containing running tasks.
const ACTIVE_FILENAME: &str = "active";
/// Filename prefix for archive files.
const ARCHIVE_FILENAME_PREFIX: &str = "archive.";
/// Filename for the state file.
const STATE_FILENAME: &str = "state";
/// Filename of the archive lockfile.
const LOCKFILE_FILENAME: &str = ".lock";
/// Write-ahead log.
const WAL_FILENAME: &str = "journal";

/// File name extension for zstd compressed archive files
const ZSTD_EXTENSION_WITH_DOT: &str = ".zst";

/// Item which can be put into the task cache.
#[derive(Clone, Debug, Serialize, Deserialize, Hash, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct TaskCacheItem {
    /// The task's UPID
    pub upid: RemoteUpid,
    /// The time at which the task was started (seconds since the UNIX epoch).
    /// Technically this is also contained within the UPID, duplicating it here
    /// allows us to directly access it when sorting in new tasks, without having
    /// to parse the UPID.
    pub starttime: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The task's status.
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// The task's endtime (seconds since the UNIX epoch).
    pub endtime: Option<i64>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
/// Per remote state.
struct RemoteState {
    /// Per-node state for this remote.
    node_state: HashMap<String, NodeState>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
struct NodeState {
    /// Cutoff timestamp for this node when fetching archived tasks.
    cutoff: i64,
}

/// State needed for task polling.
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct State {
    /// Map of remote -> most recent task starttime (UNIX epoch) in the archive.
    /// This can be used as a cut-off when requesting new task data.
    #[serde(default)]
    remote_state: HashMap<String, RemoteState>,
    /// Tracked tasks which are polled in short intervals.
    #[serde(default)]
    tracked_tasks: HashSet<RemoteUpid>,
}

impl State {
    /// Get tracked tasks.
    pub fn tracked_tasks(&self) -> impl Iterator<Item = &RemoteUpid> {
        self.tracked_tasks.iter()
    }

    /// Get the cutoff timestamp for a node of a remote.
    pub fn cutoff_timestamp(&self, remote_id: &str, node: &str) -> Option<i64> {
        self.remote_state
            .get(remote_id)
            .and_then(|remote_state| remote_state.node_state.get(node))
            .map(|state| state.cutoff)
    }

    /// Add a new tracked task.
    fn add_tracked_task(&mut self, upid: RemoteUpid) {
        self.tracked_tasks.insert(upid);
    }

    /// Remove a tracked task.
    fn remove_tracked_task(&mut self, upid: &RemoteUpid) {
        self.tracked_tasks.remove(upid);
    }

    /// Update the per-node cutoff timestamp if it is higher than the current one.
    fn update_cutoff_timestamp(&mut self, remote_id: &str, node: &str, starttime: i64) {
        match self.remote_state.get_mut(remote_id) {
            Some(remote_state) => match remote_state.node_state.get_mut(node) {
                Some(node_state) => {
                    node_state.cutoff = node_state.cutoff.max(starttime);
                }
                None => {
                    remote_state
                        .node_state
                        .insert(node.to_string(), NodeState { cutoff: starttime });
                }
            },
            None => {
                let node_state =
                    HashMap::from_iter([(node.to_string(), NodeState { cutoff: starttime })]);

                self.remote_state
                    .insert(remote_id.to_string(), RemoteState { node_state });
            }
        }
    }
}

/// Cache for remote tasks.
#[derive(Clone)]
pub struct TaskCache {
    /// Path where the cache's files should be placed.
    base_path: PathBuf,
    /// File permissions for the cache's files.
    create_options: CreateOptions,

    /// Maximum size of the journal. If it grows larger than size after
    /// tasks have been added, it will be applied immediately.
    journal_max_size: u64,

    /// Maximum number of archive files. If the archive is rotated and `max_files` is exceeded, the
    /// oldest fill we dropped
    max_files: u32,

    /// Number of uncompressed archive files to keep. These will be the most recent ones.
    uncompressed_files: u32,

    /// Rotate archive file if it is older than this number of seconds.
    rotate_after: u64,
}

/// A [`TaskCache`] locked for writing.
pub struct WritableTaskCache {
    cache: TaskCache,
    lock: TaskCacheLock,
}

/// A [`TaskCache`] locked for reading.
pub struct ReadableTaskCache {
    cache: TaskCache,
    lock: TaskCacheLock,
}

/// Lock for the cache.
#[allow(dead_code)]
struct TaskCacheLock(File);

/// Which tasks to fetch from the archive.
pub enum GetTasks {
    /// Get all tasks, finished and running.
    All,
    /// Only get running (active) tasks.
    Active,
    #[cfg(test)] // Used by tests, might be used by production code in the future
    /// Only get finished (archived) tasks.
    Archived,
}

/// Map that stores whether a remote node's tasks were successfully
/// fetched.
#[derive(Default)]
pub struct NodeFetchSuccessMap(HashMap<(String, String), bool>);

impl NodeFetchSuccessMap {
    /// Mark a node of a given remote as successful.
    pub fn set_node_success(&mut self, remote: String, node: String) {
        self.0.insert((remote, node), true);
    }

    /// Mark a node of a given remote as failed.
    pub fn set_node_failure(&mut self, remote: String, node: String) {
        self.0.insert((remote, node), false);
    }

    /// Returns whether tasks from a given node of a remote were successfully fetched.
    pub fn node_successful(&self, remote: &str, node: &str) -> bool {
        matches!(self.0.get(&(remote.into(), node.into())), Some(true))
    }

    /// Merge this map with another.
    pub fn merge(&mut self, other: Self) {
        self.0.extend(other.0);
    }
}

impl ReadableTaskCache {
    /// Iterate over cached tasks.
    pub fn get_tasks(&self, mode: GetTasks) -> Result<TaskArchiveIterator<'_>, Error> {
        self.cache
            .get_tasks_impl(mode, &self.lock)
            .context("failed to create task archive iterator")
    }
}

impl WritableTaskCache {
    /// Create initial task archive files that can be backfilled with the
    /// recent task history from a remote.
    ///
    /// This function only has an effect if there are no archive files yet.
    pub fn init(&self, now: i64) -> Result<(), Error> {
        let active_filename = self.cache.base_path.join(ACTIVE_FILENAME);

        if !active_filename.exists() {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .open(&active_filename)?;

            self.cache
                .create_options
                .apply_to(&mut file, &active_filename)?;
        }

        if self.cache.archive_files(&self.lock)?.is_empty() {
            for i in 0..self.cache.max_files {
                self.new_file(
                    now - (i as u64 * self.cache.rotate_after) as i64,
                    i >= self.cache.uncompressed_files,
                )?;
            }
        }

        Ok(())
    }

    /// Start a new archive file with a given timestamp.
    /// `now` is supposed to be a UNIX timestamp (seconds).
    fn new_file(&self, now: i64, compress: bool) -> Result<ArchiveFile, Error> {
        let suffix = if compress {
            ZSTD_EXTENSION_WITH_DOT
        } else {
            ""
        };

        let new_path = self
            .cache
            .base_path
            .join(format!("{ARCHIVE_FILENAME_PREFIX}{now}{suffix}"));

        let mut file = File::create(&new_path)?;
        self.cache.create_options.apply_to(&mut file, &new_path)?;

        if compress {
            let encoder = zstd::stream::write::Encoder::new(file, zstd::DEFAULT_COMPRESSION_LEVEL)?;
            encoder.finish()?;
        }

        Ok(ArchiveFile {
            path: new_path,
            compressed: compress,
            starttime: now,
        })
    }

    /// Rotate task archive if the the newest archive file is older than `rotate_after`.
    ///
    /// The oldest archive files are removed if the total number of archive files exceeds
    /// `max_files`. `now` is supposed to be a UNIX timestamp (seconds).
    pub fn rotate(&self, now: i64) -> Result<bool, Error> {
        let mut did_rotate = false;
        let mut archive_files = self.cache.archive_files(&self.lock)?;

        let mut start_new_file = |files: &mut Vec<ArchiveFile>| -> Result<(), Error> {
            let new_file = self.new_file(now, self.cache.uncompressed_files == 0)?;
            files.insert(0, new_file);
            self.apply_journal()?;

            did_rotate = true;
            Ok(())
        };

        match archive_files.first() {
            Some(bound) => {
                if now > bound.starttime && now - bound.starttime > self.cache.rotate_after as i64 {
                    start_new_file(&mut archive_files)?;
                }
            }
            None => start_new_file(&mut archive_files)?,
        }

        while archive_files.len() > self.cache.max_files as usize {
            // Unwrap is safe because of the length check above
            let to_remove = archive_files.pop().unwrap();
            std::fs::remove_file(&to_remove.path)
                .with_context(|| format!("failed to remove {}", to_remove.path.display()))?;
        }

        for file in archive_files
            .iter_mut()
            .skip(self.cache.uncompressed_files as usize)
        {
            if !file.compressed {
                file.compress(self.cache.create_options)
                    .with_context(|| format!("failed to compress {}", file.path.display()))?;
            }
        }

        Ok(did_rotate)
    }

    /// Iterate over cached tasks.
    pub fn get_tasks(&self, mode: GetTasks) -> Result<TaskArchiveIterator<'_>, Error> {
        self.cache
            .get_tasks_impl(mode, &self.lock)
            .context("failed to create task archive iterator")
    }

    /// Update task cache contents.
    ///
    /// This is mostly used for adding new tasks to tasks to the cache, but
    /// will also handle dropping finished/failed tracked tasks from the
    /// state file and active file. This is done so that we don't have to update
    /// these files multiple times.
    ///
    /// Running tasks (tasks without an endtime) are placed into the 'active' file in the
    /// task cache base directory. Finished tasks are sorted into `archive.<startime>` archive
    /// files, where `<starttimes>` denotes the lowest permitted start time timestamp for a given
    /// archive file. If a task which was added as running previously is added again, this time in
    /// a finished state, it will be removed from the `active` file and also sorted into
    /// one of the archive files.
    /// Same goes for the list of tracked tasks; the entry in the state file will be removed.
    ///
    /// Crash consistency:
    ///
    /// The state file, which contains the cut-off timestamps for future task fetching, is updated at the
    /// end after all tasks have been added into the archive. Adding tasks is an idempotent
    /// operation; adding the *same* task multiple times does not lead to duplicated entries in the
    /// task archive. Individual archive files are updated atomically, but since
    /// adding tasks can involve updating multiple archive files, the archive could end up
    /// in a partially-updated, inconsistent state in case of a crash.
    /// However, since the state file with the cut-off timestamps is updated last,
    /// the consistency of the archive should be restored at the next update cycle of the archive.
    pub fn update(
        &self,
        new_tasks: Vec<TaskCacheItem>,
        update_state_for_remote: &NodeFetchSuccessMap,
        drop_tracked: HashSet<RemoteUpid>,
    ) -> Result<(), Error> {
        let task_iter = self
            .get_tasks(GetTasks::Active)
            .context("failed to create archive iterator for active tasks")?;

        let mut active_tasks = HashMap::from_iter(task_iter.filter_map(|task| {
            if !drop_tracked.contains(&task.upid) {
                Some((task.upid.clone(), task))
            } else {
                None
            }
        }));

        let mut new_finished_tasks = Vec::new();

        for task in new_tasks {
            if task.endtime.is_none() {
                active_tasks.insert(task.upid.clone(), task);
            } else {
                new_finished_tasks.push(task);
            }
        }

        let mut state = self.read_state();

        for upid in drop_tracked {
            state.remove_tracked_task(&upid);
        }

        self.write_tasks_to_journal(
            new_finished_tasks,
            &mut active_tasks,
            update_state_for_remote,
            &mut state,
        )?;

        let mut active: Vec<TaskCacheItem> = active_tasks.into_values().collect();

        active.sort_by(compare_tasks_reverse);
        self.write_active_tasks(active.into_iter())
            .context("failed to write active task file when adding tasks")?;
        self.write_state(state)
            .context("failed to update task archive state file when adding tasks")?;

        self.apply_journal_if_too_large()
            .context("could not apply journal early")?;

        Ok(())
    }

    fn write_tasks_to_journal(
        &self,
        tasks: Vec<TaskCacheItem>,
        active_tasks: &mut HashMap<RemoteUpid, TaskCacheItem>,
        node_success_map: &NodeFetchSuccessMap,
        state: &mut State,
    ) -> Result<(), Error> {
        let filename = self.cache.base_path.join(WAL_FILENAME);
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(filename)?;

        for task in tasks {
            // Remove this finished task from our set of active tasks.
            active_tasks.remove(&task.upid);

            // TODO:: Handle PBS tasks correctly.
            // TODO: This is awkward, maybe overhaul RemoteUpid type to make this easier
            match task.upid.upid.parse::<PveUpid>() {
                Ok(upid) => {
                    let node = &upid.node;
                    let remote = task.upid.remote();

                    if node_success_map.node_successful(remote, node) {
                        state.update_cutoff_timestamp(task.upid.remote(), node, task.starttime);
                    }
                }
                Err(error) => {
                    log::error!("could not parse PVE UPID - not saving to task cache: {error:#}");
                    continue;
                }
            }

            serde_json::to_writer(&mut file, &task)?;
            writeln!(&file)?;
        }

        file.sync_all()?;

        Ok(())
    }

    /// Returns the current size of the journal file.
    fn journal_size(&self) -> Result<u64, Error> {
        let metadata = self
            .cache
            .base_path
            .join(WAL_FILENAME)
            .metadata()
            .context("failed to read metadata of journal file")?;

        Ok(metadata.size())
    }

    /// Apply the journal early if it has grown larger than the maximum allowed size.
    fn apply_journal_if_too_large(&self) -> Result<(), Error> {
        let size = self.journal_size()?;

        if size > self.cache.journal_max_size {
            log::info!("task cache journal size {size} bytes, applying early");
            self.apply_journal()?;
        }

        Ok(())
    }

    /// Apply the task journal.
    ///
    /// This will merge all tasks in the journal file into the task archive.
    pub fn apply_journal(&self) -> Result<(), Error> {
        let start = Instant::now();
        let filename = self.cache.base_path.join(WAL_FILENAME);

        let file = match File::open(&filename) {
            Ok(file) => Box::new(BufReader::new(file)),
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err.into()),
        };

        log::info!("applying task cache journal");
        let iterator = ArchiveIterator::new(file);

        let mut tasks: Vec<TaskCacheItem> = iterator
            .filter_map(|task| match task {
                Ok(task) => Some(task),
                Err(err) => {
                    log::error!("could not read task from journal file: {err:#}");
                    None
                }
            })
            .collect();

        // The WAL contains tasks in arbitrary order since we always append.
        tasks.sort_by(compare_tasks_reverse);
        tasks.dedup();

        let count = tasks.len();

        self.merge_tasks_into_archive(tasks)?;

        // truncate the journal file
        OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(filename)
            .context("failed to truncate journal file")?;

        log::info!(
            "commited {count} tasks in {:.3}.s to task cache archive",
            start.elapsed().as_secs_f32()
        );

        Ok(())
    }

    /// Merge a list of *finished* tasks into the remote task archive files.
    /// The list of task in `tasks` *must* be sorted by their timestamp and UPID (descending by
    /// timestamp, ascending by UPID).
    fn merge_tasks_into_archive(&self, tasks: Vec<TaskCacheItem>) -> Result<(), Error> {
        debug_assert!(tasks
            .iter()
            .is_sorted_by(|a, b| compare_tasks(a, b).is_ge()));

        let files = self
            .cache
            .archive_files(&self.lock)
            .context("failed to read achive files")?;

        let mut files = files.iter().peekable();

        let mut current = files.next();
        let mut next = files.peek();

        let mut tasks_for_current_file = Vec::new();

        // Tasks are sorted youngest to oldest (biggest start time first)
        for task in tasks {
            // Skip ahead until we have found the correct file.
            while next.is_some() {
                if let Some(current) = current {
                    if task.starttime >= current.starttime {
                        break;
                    }
                    // The next entry's cut-off is larger then the task's start time, that means
                    // we want to finalized the current file by merging all tasks that
                    // should be stored in it...
                    self.merge_single_archive_file(
                        std::mem::take(&mut tasks_for_current_file),
                        current,
                    )
                    .with_context(|| {
                        format!("failed to merge archive file {}", current.path.display())
                    })?;
                }

                // ... and the `current` file to the next entry.
                current = files.next();
                next = files.peek();
            }

            if let Some(current) = current {
                if task.starttime < current.starttime {
                    continue;
                }
            }
            tasks_for_current_file.push(task);
        }

        // Merge tasks for the last file.
        if let Some(current) = current {
            self.merge_single_archive_file(tasks_for_current_file, current)
                .with_context(|| {
                    format!("failed to merge archive file {}", current.path.display())
                })?;
        }

        Ok(())
    }

    /// Add a new tracked task.
    ///
    /// This will insert the task in the list of tracked tasks in the state file,
    /// as well as create an entry in the `active` file.
    pub fn add_tracked_task(&self, task: TaskCacheItem) -> Result<(), Error> {
        let mut state = self.read_state();

        let mut tasks: Vec<TaskCacheItem> = self
            .get_tasks(GetTasks::Active)
            .context("failed to create active task iterator")?
            .collect();

        tasks.push(task.clone());
        tasks.sort_by(compare_tasks_reverse);

        state.add_tracked_task(task.upid);

        self.write_active_tasks(tasks.into_iter())
            .context("failed to write active tasks file when adding tracked task")?;

        self.write_state(state)
            .context("failed to write state when adding tracked task")?;

        Ok(())
    }

    /// Read the state file.
    /// If the state file could not be read or does not exist, the default (empty) state
    /// is returned.
    pub fn read_state(&self) -> State {
        self.cache.read_state()
    }

    /// Write the state file.
    fn write_state(&self, state: State) -> Result<(), Error> {
        let path = self.cache.base_path.join(STATE_FILENAME);

        let data = serde_json::to_vec_pretty(&state)?;

        proxmox_sys::fs::replace_file(path, &data, self.cache.create_options, true)?;

        Ok(())
    }

    /// Write the provided tasks to the 'active' file.
    ///
    /// The tasks are first written to a temporary file, which is then used
    /// to atomically replace the original.
    fn write_active_tasks(&self, tasks: impl Iterator<Item = TaskCacheItem>) -> Result<(), Error> {
        let (fd, path) = proxmox_sys::fs::make_tmp_file(
            self.cache.base_path.join(ACTIVE_FILENAME),
            self.cache.create_options,
        )?;
        let mut fd = BufWriter::new(fd);

        Self::write_tasks(&mut fd, tasks)?;

        if let Err(err) = fd.flush() {
            log::error!("could not flush 'active' file: {err:#}");
        }
        drop(fd);

        let target = self.cache.base_path.join(ACTIVE_FILENAME);

        let res = std::fs::rename(&path, &target).with_context(|| {
            format!(
                "failed to replace {} with {}",
                target.display(),
                path.display(),
            )
        });

        if let Err(err) = res {
            if let Err(err) = std::fs::remove_file(&path) {
                log::error!(
                    "failed to cleanup temporary file {}: {err:#}",
                    path.display()
                );
            }

            return Err(err);
        }

        Ok(())
    }

    /// Merge `tasks` with an existing archive file.
    /// This function assumes that `tasks` and the pre-existing contents of the archive
    /// file are both sorted descending by starttime (most recent tasks come first).
    /// The task archive must be locked when calling this function.
    fn merge_single_archive_file(
        &self,
        tasks: Vec<TaskCacheItem>,
        file: &ArchiveFile,
    ) -> Result<(), Error> {
        if tasks.is_empty() {
            return Ok(());
        }

        // TODO: Might be nice to also move this to ArchiveFile
        let (temp_file, temp_file_path) =
            proxmox_sys::fs::make_tmp_file(&file.path, self.cache.create_options)?;
        let mut writer = if file.compressed {
            let encoder =
                zstd::stream::write::Encoder::new(temp_file, zstd::DEFAULT_COMPRESSION_LEVEL)?
                    .auto_finish();
            Box::new(BufWriter::new(encoder)) as Box<dyn Write>
        } else {
            Box::new(BufWriter::new(temp_file)) as Box<dyn Write>
        };

        let archive_iter = file
            .iter()?
            .flat_map(|item| match item {
                Ok(item) => Some(item),
                Err(err) => {
                    log::error!("could not read task cache item while merging: {err:#}");
                    None
                }
            })
            .peekable();
        let task_iter = tasks.into_iter().peekable();

        Self::write_tasks(&mut writer, MergeTaskIterator::new(archive_iter, task_iter))?;

        if let Err(err) = writer.flush() {
            log::error!("could not flush BufWriter for {file:?}: {err:#}");
        }
        drop(writer);

        if let Err(err) = std::fs::rename(&temp_file_path, &file.path).with_context(|| {
            format!(
                "failed to replace {} with {}",
                file.path.display(),
                temp_file_path.display()
            )
        }) {
            if let Err(err) = std::fs::remove_file(&temp_file_path) {
                log::error!(
                    "failed to clean up temporary file {}: {err:#}",
                    temp_file_path.display()
                );
            }

            return Err(err);
        }

        Ok(())
    }

    /// Write an iterator of [`TaskCacheItem`] to a something that implements [`Write`].
    /// The individual items are encoded as JSON followed by a newline.
    fn write_tasks(
        writer: &mut impl Write,
        tasks: impl Iterator<Item = TaskCacheItem>,
    ) -> Result<(), Error> {
        for task in tasks {
            serde_json::to_writer(&mut *writer, &task)?;
            writeln!(writer)?;
        }

        Ok(())
    }
}

impl TaskCache {
    /// Create a new task cache instance.
    ///
    /// Remember to call `init` or `new_file` on a locked, writable TaskCache
    /// to create the initial archive files.
    pub fn new<P: AsRef<Path>>(
        path: P,
        create_options: CreateOptions,
        max_files: u32,
        uncompressed: u32,
        rotate_after: u64,
        journal_max_size: u64,
    ) -> Result<Self, Error> {
        Ok(Self {
            base_path: path.as_ref().into(),
            create_options,
            journal_max_size,
            max_files,
            rotate_after,
            uncompressed_files: uncompressed,
        })
    }

    /// Lock the cache for reading.
    pub fn read(self) -> Result<ReadableTaskCache, Error> {
        let lock = self.lock_impl(false)?;

        Ok(ReadableTaskCache { cache: self, lock })
    }

    /// Lock the cache for writing.
    pub fn write(self) -> Result<WritableTaskCache, Error> {
        let lock = self.lock_impl(true)?;

        Ok(WritableTaskCache { cache: self, lock })
    }

    fn lock_impl(&self, exclusive: bool) -> Result<TaskCacheLock, Error> {
        let lockfile = self.base_path.join(LOCKFILE_FILENAME);

        Ok(TaskCacheLock(proxmox_sys::fs::open_file_locked(
            lockfile,
            Duration::from_secs(10),
            exclusive,
            self.create_options,
        )?))
    }

    /// Read the state file.
    /// If the state file could not be read or does not exist, the default (empty) state
    /// is returned.
    pub fn read_state(&self) -> State {
        fn do_read_state(path: &Path) -> Result<State, Error> {
            match std::fs::read(path) {
                Ok(content) => serde_json::from_slice(&content).map_err(|err| err.into()),
                Err(err) if err.kind() == ErrorKind::NotFound => Ok(Default::default()),
                Err(err) => Err(err.into()),
            }
        }

        let path = self.base_path.join(STATE_FILENAME);
        do_read_state(&path).unwrap_or_else(|err| {
            log::error!("could not read state file: {err:#}");
            Default::default()
        })
    }

    fn get_tasks_impl<'a>(
        &self,
        mode: GetTasks,
        lock: &'a TaskCacheLock,
    ) -> Result<TaskArchiveIterator<'a>, Error> {
        let journal_file = self.base_path.join(WAL_FILENAME);
        let active_path = self.base_path.join(ACTIVE_FILENAME);

        match mode {
            GetTasks::All => {
                let mut archive_files = self.archive_files(lock)?;
                archive_files.reverse();

                if active_path.exists() {
                    archive_files.push(ArchiveFile {
                        path: self.base_path.join(ACTIVE_FILENAME),
                        compressed: false,
                        starttime: 0,
                    });
                }

                TaskArchiveIterator::new(Some(journal_file), archive_files, lock)
            }
            GetTasks::Active => {
                let mut archive_files = Vec::new();

                if active_path.exists() {
                    archive_files.push(ArchiveFile {
                        path: self.base_path.join(ACTIVE_FILENAME),
                        compressed: false,
                        starttime: 0,
                    });
                }

                TaskArchiveIterator::new(None, archive_files, lock)
            }
            #[cfg(test)]
            GetTasks::Archived => {
                let mut files = self.archive_files(lock)?;
                files.reverse();

                TaskArchiveIterator::new(Some(journal_file), files, lock)
            }
        }
    }

    /// Returns a list of existing archive files, together with their respective
    /// cut-off timestamp. The result is sorted ascending by cut-off timestamp (most recent one
    /// first).
    /// The task archive should be locked for reading when calling this function.
    fn archive_files(&self, _lock: &TaskCacheLock) -> Result<Vec<ArchiveFile>, Error> {
        let mut names = Vec::new();

        for entry in std::fs::read_dir(&self.base_path)? {
            let entry = entry?;

            let path = entry.path();

            if let Some(file) = Self::parse_archive_filename(&path) {
                names.push(file);
            }
        }

        names.sort_by_key(|e| -e.starttime);

        Ok(names)
    }

    fn parse_archive_filename(path: &Path) -> Option<ArchiveFile> {
        let filename = path.file_name()?.to_str()?;
        let filename = filename.strip_prefix(ARCHIVE_FILENAME_PREFIX)?;

        if let Some(starttime) = filename.strip_suffix(ZSTD_EXTENSION_WITH_DOT) {
            let starttime: i64 = starttime.parse().ok()?;

            Some(ArchiveFile {
                path: path.to_path_buf(),
                compressed: true,
                starttime,
            })
        } else {
            let starttime: i64 = filename.parse().ok()?;

            Some(ArchiveFile {
                path: path.to_path_buf(),
                compressed: false,
                starttime,
            })
        }
    }
}

/// Comparison function for sorting tasks.
/// The tasks are compared based on the task's start time, falling
/// back to the task's UPID as a secondary criterion in case the
/// start times are equal.
fn compare_tasks(a: &TaskCacheItem, b: &TaskCacheItem) -> Ordering {
    a.starttime
        .cmp(&b.starttime)
        .then_with(|| b.upid.to_string().cmp(&a.upid.to_string()))
}

/// Comparison function for sorting tasks, reversed
/// The tasks are compared based on the task's start time, falling
/// back to the task's UPID as a secondary criterion in case the
/// start times are equal.
fn compare_tasks_reverse(a: &TaskCacheItem, b: &TaskCacheItem) -> Ordering {
    compare_tasks(b, a)
}

/// Iterator over the task archive.
pub struct TaskArchiveIterator<'a> {
    inner: Box<dyn Iterator<Item = TaskCacheItem>>,

    /// Lock for this archive. This contains the lock in case we
    /// need to keep the archive locked while iterating over it.
    _lock: &'a TaskCacheLock,
}

impl<'a> TaskArchiveIterator<'a> {
    /// Create a new task archive iterator.
    ///
    /// `files` should be sorted with the most recent archive file *last*.
    fn new(
        journal: Option<PathBuf>,
        files: Vec<ArchiveFile>,
        lock: &'a TaskCacheLock,
    ) -> Result<Self, Error> {
        let inner = InnerTaskArchiveIterator::new(files)
            .filter_map(|res| match res {
                Ok(task) => Some(task),
                Err(err) => {
                    log::error!("could not read task from archive file: {err:#}");
                    None
                }
            })
            .peekable();

        if let Some(journal) = journal {
            let journal_reader = Box::new(BufReader::new(File::open(journal)?));
            let journal_task_iterator = JournalIterator::new(journal_reader).peekable();
            let merge_task_iter = MergeTaskIterator::new(journal_task_iterator, inner);

            Ok(Self {
                inner: Box::new(merge_task_iter),
                _lock: lock,
            })
        } else {
            Ok(Self {
                inner: Box::new(inner),
                _lock: lock,
            })
        }
    }
}

impl Iterator for TaskArchiveIterator<'_> {
    type Item = TaskCacheItem;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

struct InnerTaskArchiveIterator {
    /// Archive files to read.
    files: Vec<ArchiveFile>,
    /// Archive iterator we are currently using, if any
    current: Option<ArchiveIterator>,
}

impl InnerTaskArchiveIterator {
    /// Create a new task archive iterator.
    pub fn new(files: Vec<ArchiveFile>) -> Self {
        Self {
            files,
            current: None,
        }
    }
}

impl Iterator for InnerTaskArchiveIterator {
    type Item = Result<TaskCacheItem, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match &mut self.current {
                Some(current) => {
                    let next = current.next();
                    if next.is_some() {
                        return next;
                    } else {
                        self.current = None;
                    }
                }
                None => 'inner: loop {
                    // Returns `None` if no more files are available, stopping iteration.
                    let next_file = self.files.pop()?;

                    match next_file.iter() {
                        Ok(iter) => {
                            self.current = Some(iter);
                            break 'inner;
                        }
                        Err(err) => {
                            log::error!("could not create archive iterator while iteration over task archive files, skipping: {err:#}")
                        }
                    }
                },
            }
        }
    }
}

/// Archive file.
#[derive(Clone, Debug)]
struct ArchiveFile {
    /// The path to the archive file.
    path: PathBuf,
    /// This archive file is compressed using zstd.
    compressed: bool,
    /// The archive's lowest permitted starttime (seconds since UNIX epoch).
    starttime: i64,
}

impl ArchiveFile {
    /// Create an [`ArchiveIterator`] for this file.
    fn iter(&self) -> Result<ArchiveIterator, Error> {
        let fd = File::open(&self.path)
            .with_context(|| format!("failed to open archive file {}", self.path.display()))?;

        let iter = if self.compressed {
            let reader = zstd::stream::read::Decoder::new(fd).with_context(|| {
                format!(
                    "failed to create zstd decoder for archive file {}",
                    self.path.display()
                )
            })?;
            ArchiveIterator::new(Box::new(BufReader::new(reader)))
        } else {
            ArchiveIterator::new(Box::new(BufReader::new(fd)))
        };

        Ok(iter)
    }

    fn compress(&mut self, options: CreateOptions) -> Result<(), Error> {
        let uncompressed_file_path = &self.path;

        let (temp_file, temp_file_path) =
            proxmox_sys::fs::make_tmp_file(uncompressed_file_path, options)
                .context("failed to create temporary file")?;

        let uncompressed_file =
            File::open(uncompressed_file_path).context("failed to open uncompressed file")?;

        zstd::stream::copy_encode(
            uncompressed_file,
            temp_file,
            zstd::DEFAULT_COMPRESSION_LEVEL,
        )
        .context("zstd::stream::copy_encode failed")?;

        let mut new_path_for_compressed = uncompressed_file_path.clone();
        new_path_for_compressed
            .set_extension(format!("{}{ZSTD_EXTENSION_WITH_DOT}", self.starttime));

        std::fs::rename(&temp_file_path, &new_path_for_compressed)
            .context("failed to move compressed task achive file")?;
        std::fs::remove_file(uncompressed_file_path)
            .context("failed to remove uncompressed archive file")?;

        self.path = new_path_for_compressed;
        self.compressed = true;

        Ok(())
    }
}

/// Iterator that merges two _sorted_ `Iterator<Item = TaskCacheItem>`, returning the items
/// from both iterators sorted.
/// The two iterators are expected to be sorted descendingly based on the task's starttime and
/// ascendingly based on the task's UPID's string representation. This can be
/// achieved by using the [`compare_tasks_reverse`] function when sorting an array of tasks.
struct MergeTaskIterator<T: Iterator<Item = TaskCacheItem>, U: Iterator<Item = TaskCacheItem>> {
    left: Peekable<T>,
    right: Peekable<U>,
}

impl<T, U> MergeTaskIterator<T, U>
where
    T: Iterator<Item = TaskCacheItem>,
    U: Iterator<Item = TaskCacheItem>,
{
    /// Create a new merging iterator.
    fn new(left: Peekable<T>, right: Peekable<U>) -> Self {
        Self { left, right }
    }
}

impl<T, U> Iterator for MergeTaskIterator<T, U>
where
    T: Iterator<Item = TaskCacheItem>,
    U: Iterator<Item = TaskCacheItem>,
{
    type Item = T::Item;

    fn next(&mut self) -> Option<T::Item> {
        let order = match (self.left.peek(), self.right.peek()) {
            (Some(l), Some(r)) => Some(compare_tasks(l, r)),
            (Some(_), None) => Some(Ordering::Greater),
            (None, Some(_)) => Some(Ordering::Less),
            (None, None) => None,
        };

        match order {
            Some(Ordering::Greater) => self.left.next(),
            Some(Ordering::Less) => self.right.next(),
            Some(Ordering::Equal) => {
                // Dedup by consuming the other iterator as well
                let _ = self.right.next();
                self.left.next()
            }
            None => None,
        }
    }
}

/// Iterator for a single task archive file.
///
/// This iterator implements `Iterator<Item = Result<TaskCacheItem, Error>`. When iterating,
/// tasks are read line by line, without leading the entire archive file into memory.
struct ArchiveIterator {
    iter: Lines<Box<dyn BufRead>>,
}

impl ArchiveIterator {
    /// Create a new iterator.
    pub fn new(reader: Box<dyn BufRead>) -> Self {
        let lines = reader.lines();

        Self { iter: lines }
    }
}

impl Iterator for ArchiveIterator {
    type Item = Result<TaskCacheItem, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|result| {
            result
                .and_then(|line| Ok(serde_json::from_str(&line)?))
                .map_err(Into::into)
        })
    }
}

/// Iterator for journal files. This iterator uses [`ArchiveIterator`] internally, but will eagerly
/// load all tasks into memory to sort and deduplicate them.
struct JournalIterator {
    inner: Box<dyn Iterator<Item = TaskCacheItem>>,
}

impl JournalIterator {
    fn new(file: Box<dyn BufRead>) -> Self {
        let iter = ArchiveIterator::new(file);

        let mut tasks: Vec<TaskCacheItem> = iter
            .flat_map(|task| match task {
                Ok(task) => Some(task),
                Err(err) => {
                    log::error!("could not read task while iterating over archive file: {err:#}");
                    None
                }
            })
            .collect();

        tasks.sort_by(compare_tasks_reverse);
        tasks.dedup();

        Self {
            inner: Box::new(tasks.into_iter()),
        }
    }
}

impl Iterator for JournalIterator {
    type Item = TaskCacheItem;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::test_support::temp::NamedTempDir;

    use super::*;

    #[test]
    fn archive_filename() {
        let a = TaskCache::parse_archive_filename(&PathBuf::from("/tmp/archive.10000")).unwrap();

        assert_eq!(a.path, PathBuf::from("/tmp/archive.10000"));
        assert_eq!(a.starttime, 10000);
        assert!(!a.compressed);

        let a = TaskCache::parse_archive_filename(&PathBuf::from("/tmp/archive.1234.zst")).unwrap();

        assert_eq!(a.path, PathBuf::from("/tmp/archive.1234.zst"));
        assert_eq!(a.starttime, 1234);
        assert!(a.compressed);
    }

    #[test]
    fn archive_iterator() -> Result<(), Error> {
        let file = r#"
            {"upid":"pve-remote!UPID:pve:00039E4D:002638B8:67B4A9D1:stopall::root@pam:","status":"OK","endtime":12345, "starttime": 1234}
            {"upid":"pbs-remote!UPID:pbs:000002B2:00000158:00000000:674D828C:logrotate::root@pam:","status":"OK","endtime":12345, "starttime": 1234}
            invalid"#
            .trim_start();

        let cursor = Box::new(Cursor::new(file.as_bytes()));
        let mut iter = ArchiveIterator::new(cursor);

        assert_eq!(iter.next().unwrap().unwrap().upid.remote(), "pve-remote");
        assert_eq!(iter.next().unwrap().unwrap().upid.remote(), "pbs-remote");
        assert!(iter.next().unwrap().is_err());
        assert!(iter.next().is_none());

        Ok(())
    }

    fn task(starttime: i64, ended: bool) -> TaskCacheItem {
        let (status, endtime) = if ended {
            (Some("OK".into()), Some(starttime + 10))
        } else {
            (None, None)
        };

        TaskCacheItem {
            upid: format!(
                "pve-remote!UPID:pve:00039E4D:002638B8:{starttime:08X}:stopall::root@pam:"
            )
            .parse()
            .unwrap(),
            starttime,
            status,
            endtime,
        }
    }

    fn assert_starttimes(cache: &WritableTaskCache, starttimes: &[i64]) {
        let tasks: Vec<i64> = cache
            .get_tasks(GetTasks::All)
            .unwrap()
            .map(|task| task.starttime)
            .collect();

        assert_eq!(&tasks, starttimes);
    }

    fn add_tasks(cache: &WritableTaskCache, tasks: Vec<TaskCacheItem>) -> Result<(), Error> {
        let mut node_map = NodeFetchSuccessMap::default();
        node_map.set_node_success("pve-remote".to_string(), "pve".to_string());

        cache.update(tasks, &node_map, HashSet::new())
    }

    const DEFAULT_MAX_SIZE: u64 = 10000;

    #[test]
    fn test_add_tasks() -> Result<(), Error> {
        let tmp_dir = NamedTempDir::new()?;
        let cache = TaskCache::new(
            tmp_dir.path(),
            CreateOptions::new(),
            3,
            1,
            0,
            DEFAULT_MAX_SIZE,
        )
        .unwrap()
        .write()?;

        cache.new_file(1000, false)?;
        assert_eq!(cache.cache.archive_files(&cache.lock)?.len(), 1);

        add_tasks(&cache, vec![task(1000, true), task(1001, true)])?;

        assert_eq!(
            cache.read_state().cutoff_timestamp("pve-remote", "pve"),
            Some(1001)
        );

        cache.rotate(1500)?;

        assert_eq!(cache.cache.archive_files(&cache.lock)?.len(), 2);

        add_tasks(&cache, vec![task(1500, true), task(1501, true)])?;
        add_tasks(&cache, vec![task(1200, true), task(1300, true)])?;

        assert_eq!(
            cache.read_state().cutoff_timestamp("pve-remote", "pve"),
            Some(1501),
        );

        cache.rotate(2000)?;
        assert_eq!(cache.cache.archive_files(&cache.lock)?.len(), 3);

        add_tasks(&cache, vec![task(2000, true)])?;
        add_tasks(&cache, vec![task(1502, true)])?;
        add_tasks(&cache, vec![task(1002, true)])?;

        // These are before the cut-off of 1000, so they will be discarded.
        // add_tasks(&cache, vec![task(800, true), task(900, true)])?;

        // This one should be deduped
        add_tasks(&cache, vec![task(1000, true)])?;

        assert_starttimes(
            &cache,
            &[2000, 1502, 1501, 1500, 1300, 1200, 1002, 1001, 1000],
        );

        cache.rotate(2500)?;

        assert_eq!(cache.cache.archive_files(&cache.lock)?.len(), 3);

        assert_starttimes(&cache, &[2000, 1502, 1501, 1500]);

        cache.rotate(3000)?;
        assert_eq!(cache.cache.archive_files(&cache.lock)?.len(), 3);

        assert_starttimes(&cache, &[2000]);

        Ok(())
    }

    #[test]
    fn test_active_tasks_are_migrated_to_archive() -> Result<(), Error> {
        let tmp_dir = NamedTempDir::new()?;
        let cache = TaskCache::new(
            tmp_dir.path(),
            CreateOptions::new(),
            3,
            1,
            0,
            DEFAULT_MAX_SIZE,
        )
        .unwrap()
        .write()?;

        cache.new_file(1000, false)?;
        add_tasks(&cache, vec![task(1000, false), task(1001, false)])?;
        assert_eq!(cache.get_tasks(GetTasks::Active)?.count(), 2);

        add_tasks(&cache, vec![task(1000, true), task(1001, true)])?;

        assert_starttimes(&cache, &[1001, 1000]);

        assert_eq!(cache.get_tasks(GetTasks::Active)?.count(), 0);

        Ok(())
    }

    #[test]
    fn test_init() -> Result<(), Error> {
        let tmp_dir = NamedTempDir::new()?;
        let cache = TaskCache::new(
            tmp_dir.path(),
            CreateOptions::new(),
            3,
            1,
            100,
            DEFAULT_MAX_SIZE,
        )
        .unwrap()
        .write()?;

        cache.init(1000)?;
        assert_eq!(cache.cache.archive_files(&cache.lock)?.len(), 3);

        add_tasks(
            &cache,
            vec![task(1050, true), task(950, true), task(850, true)],
        )?;

        assert_eq!(cache.get_tasks(GetTasks::Archived)?.count(), 3);

        Ok(())
    }

    fn add_finished_tracked(cache: &WritableTaskCache, starttime: i64) -> Result<(), Error> {
        let t = task(starttime, true);
        let upid = t.upid.clone();

        let mut node_map = NodeFetchSuccessMap::default();
        node_map.set_node_success("pve-remote".to_string(), "pve".to_string());

        cache.update(vec![t], &node_map, HashSet::from_iter([upid]))
    }

    #[test]
    fn test_tracking_tasks() -> Result<(), Error> {
        let tmp_dir = NamedTempDir::new()?;
        let cache = TaskCache::new(
            tmp_dir.path(),
            CreateOptions::new(),
            3,
            1,
            100,
            DEFAULT_MAX_SIZE,
        )
        .unwrap()
        .write()?;

        cache.init(1000)?;

        cache.add_tracked_task(task(1050, false))?;

        assert_eq!(cache.get_tasks(GetTasks::Active)?.count(), 1);
        cache.add_tracked_task(task(1060, false))?;
        assert_eq!(cache.get_tasks(GetTasks::Active)?.count(), 2);

        assert_eq!(cache.read_state().tracked_tasks().count(), 2);

        // Mark first task as finished
        add_finished_tracked(&cache, 1050)?;

        assert_eq!(cache.get_tasks(GetTasks::Active)?.count(), 1);
        assert_eq!(cache.get_tasks(GetTasks::Archived)?.count(), 1);
        assert_eq!(cache.read_state().tracked_tasks().count(), 1);

        // Mark second task as finished
        add_finished_tracked(&cache, 1060)?;

        assert_eq!(cache.get_tasks(GetTasks::Active)?.count(), 0);
        assert_eq!(cache.get_tasks(GetTasks::Archived)?.count(), 2);
        assert_eq!(cache.read_state().tracked_tasks().count(), 0);

        Ok(())
    }

    #[test]
    fn journal_is_applied_if_max_size_exceeded() -> Result<(), Error> {
        let tmp_dir = NamedTempDir::new()?;

        // Should be *just* enough to fit a single task, which means that we apply the journal
        // after adding a second one.
        const ENOUGH_FOR_SINGLE_TASK: u64 = 200;

        let cache = TaskCache::new(
            tmp_dir.path(),
            CreateOptions::new(),
            3,
            1,
            100,
            ENOUGH_FOR_SINGLE_TASK,
        )
        .unwrap()
        .write()?;

        add_tasks(&cache, vec![task(1000, true)])?;
        assert!(cache.journal_size()? > 0);

        add_tasks(&cache, vec![task(1000, true)])?;

        assert_eq!(cache.journal_size()?, 0);

        Ok(())
    }
}
