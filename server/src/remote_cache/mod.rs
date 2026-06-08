//! This currently only matters for PVE remotes.
//!
//! PVE remotes have multiple nodes which have names we cannot necessarily infer from the
//! "hostname" field, since that might be a different address, potentially with a port.
//!
//! We also do not want users to have to maintain the PDM host/node-name combinations (in case they
//! rename or reinstall nodes). Renaming would break the PDM config, reinstalling would break eg. a
//! "machine-id" based mapping.
//!
//! We also cannot rely in the TLS fingerprints, because a whole cluster could potentially use a
//! single wildcard certificate.
//!
//! Instead, we maintain a cached mapping of `address ↔ name` in `/var`, which gets polled
//! regularly.
//! For PVE we can query an address' `/cluster/status` and look for an entry marked as `local:1`.
//! Later this might be changed to looking for the node name in the result of
//! `/nodes/localhost/status` - once this is implemented and rolled out long enough in PVE.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Error};
use serde::{Deserialize, Serialize};

use proxmox_product_config::replace_config;
use proxmox_product_config::{ApiLockGuard, open_api_lockfile};
use proxmox_time::epoch_i64;

use pdm_api_types::remotes::RemoteType;
use pdm_config::ConfigVersionCache;

mod back_off;
use back_off::BackOffState;

const CACHE_FILENAME: &str = concat!(
    pdm_buildcfg::PDM_CACHE_DIR_M!(),
    "/remote-mapping-cache.json"
);

const LOCK_FILE: &str = concat!(
    pdm_buildcfg::PDM_CACHE_DIR_M!(),
    "/.remote-mapping-cache.json.lock"
);

static CURRENT_CACHE: Mutex<Option<CacheState>> = Mutex::new(None);

#[derive(Clone)]
struct CacheState {
    cache: Arc<RemoteMappingCache>,
    generation: usize,
}

impl CacheState {
    fn get() -> Self {
        let mut cache = CURRENT_CACHE.lock().unwrap();

        let version_cache = ConfigVersionCache::new_log_error();

        if let Some(cache) = cache.clone() {
            if let Some(version_cache) = version_cache.as_deref() {
                if cache.generation == version_cache.remote_mapping_cache() {
                    return cache;
                }
                // outdated, fall back to reloading
            }
            // outdated, or we failed to query the version cache, fall through to the load
        }

        // we have no valid cache yet:
        let generation = version_cache.map(|c| c.remote_mapping_cache()).unwrap_or(0);

        let data = Arc::new(RemoteMappingCache::load());
        let this = CacheState {
            cache: Arc::clone(&data),
            generation,
        };
        *cache = Some(this.clone());
        this
    }

    fn update(cache: RemoteMappingCache) {
        let mut current_cache = CURRENT_CACHE.lock().unwrap();
        let generation = match pdm_config::ConfigVersionCache::new_log_error() {
            Some(version_cache) => version_cache.increase_remote_mapping_cache(),
            None => 0,
        };
        *current_cache = Some(CacheState {
            generation,
            cache: Arc::new(cache),
        });
    }
}

pub struct WriteRemoteMappingCache {
    pub data: RemoteMappingCache,
    _lock: ApiLockGuard,
}

impl std::ops::Deref for WriteRemoteMappingCache {
    type Target = RemoteMappingCache;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl std::ops::DerefMut for WriteRemoteMappingCache {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl WriteRemoteMappingCache {
    fn new(lock: ApiLockGuard, data: RemoteMappingCache) -> Self {
        Self { _lock: lock, data }
    }

    pub fn save(self) -> Result<(), Error> {
        self.data.save()?;
        CacheState::update(self.data);
        Ok(())
    }
}

/// File format for `/var/cache/proxmox-datacenter-manager/remote-mapping-cache.json`
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct RemoteMappingCache {
    /// This maps a remote name to its mapping.
    pub remotes: HashMap<String, RemoteMapping>,

    /// A remote that is designated canary for which the back-off rules are not applied.
    /// This is used in case all remotes are marked as offline, so we have a single remote
    /// that is queried more often than the others.
    ///
    /// Used to detect total network failure (and restoration) on the PDM side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    canary_remote: Option<String>,
}

impl RemoteMappingCache {
    /// Get read only access to the current cache.
    pub fn get() -> Arc<Self> {
        Arc::clone(&CacheState::get().cache)
    }

    /// *Lock* the cache lock file and get mutable access to the current contents.
    pub fn write() -> Result<WriteRemoteMappingCache, Error> {
        let write_lock = open_api_lockfile(LOCK_FILE, None, true)?;

        Ok(WriteRemoteMappingCache::new(
            write_lock,
            Self::clone(&Self::get()),
        ))
    }

    /// Load the current remote mapping cache. This always succeeds and may return an empty cache.
    fn load() -> Self {
        fn do_load() -> Result<Option<RemoteMappingCache>, Error> {
            Ok(proxmox_sys::fs::file_read_optional_string(CACHE_FILENAME)?
                .map(|content| serde_json::from_str(&content))
                .transpose()?)
        }

        match do_load() {
            Ok(Some(data)) => return data,
            Ok(None) => (),
            Err(err) => {
                log::error!("corrupted remote-mapping-cache.json file, discarding - {err:?}");
            }
        }

        Self::default()
    }

    /// Save the current remote mapping cache. This should only be done by the remote mapping task.
    fn save(&self) -> Result<(), Error> {
        let raw = serde_json::to_vec(self).context("failed to serialize remote mapping cache")?;
        replace_config(CACHE_FILENAME, &raw)
    }

    /// Attempt to retrieve the host name from a node name.
    pub fn node_name_to_hostname(&self, remote: &str, node_name: &str) -> Option<&str> {
        Some(self.remotes.get(remote)?.node_to_host.get(node_name)?)
    }

    /// Attempt to get the node info for a node name.
    pub fn info_by_node_name(&self, remote_name: &str, node_name: &str) -> Option<&HostInfo> {
        let remote = self.remotes.get(remote_name)?;
        let host = remote.node_to_host.get(node_name)?;
        remote.hosts.get(host)
    }

    pub fn info_by_node_name_mut(
        &mut self,
        remote_name: &str,
        node_name: &str,
    ) -> Option<&mut HostInfo> {
        let remote = self.remotes.get_mut(remote_name)?;
        let host = remote.node_to_host.get(node_name)?;
        remote.hosts.get_mut(host)
    }

    /// Attempt to retrieve the node name from a host name.
    pub fn info_by_hostname(&self, remote: &str, hostname: &str) -> Option<&HostInfo> {
        self.remotes.get(remote)?.hosts.get(hostname)
    }

    pub fn info_by_hostname_mut(&mut self, remote: &str, hostname: &str) -> Option<&mut HostInfo> {
        self.remotes.get_mut(remote)?.hosts.get_mut(hostname)
    }

    // checks to see if a canary is needed and sets it,
    // and checks if we can reset all back-off states
    fn set_or_reset_canary(&mut self, remote_name: &str, unreachable: bool) {
        // if all remotes are marked offline, use this last one as canary
        if unreachable && self.canary_is_needed() {
            log::debug!("all remotes were marked unreachable, selecting {remote_name} as canary");
            self.canary_remote = Some(remote_name.to_string());
        }

        // if we marked a host (and with it a remote) as reachable and we had a canary (meaning
        // all remotes were offline at the same time) reset the whole back-off state of all remotes
        if !unreachable && self.canary_remote.is_some() {
            log::debug!(
                "{remote_name} became reachable again after all were offline, resetting all back-off states"
            );
            self.reset_all_back_off_states();
        }
    }

    /// Mark a host as reachable.
    pub fn mark_host_reachable(
        &mut self,
        remote_name: &str,
        hostname: &str,
        connection_state: ConnectionState,
    ) {
        let unreachable = matches!(&connection_state, ConnectionState::Unreachable(_));

        if let Some(info) = self.info_by_hostname_mut(remote_name, hostname) {
            info.set_reachable(connection_state);
        }

        self.set_or_reset_canary(remote_name, unreachable);
    }

    /// Mark a host as reachable.
    pub fn mark_node_reachable(
        &mut self,
        remote_name: &str,
        node_name: &str,
        connection_state: ConnectionState,
    ) {
        let unreachable = matches!(&connection_state, ConnectionState::Unreachable(_));

        if let Some(info) = self.info_by_node_name_mut(remote_name, node_name) {
            info.set_reachable(connection_state);
        }

        self.set_or_reset_canary(remote_name, unreachable);
    }

    /// Update the node name for a host, if the remote and host exist (otherwise this does
    /// nothing).
    pub fn set_node_name(&mut self, remote_name: &str, hostname: &str, node_name: Option<String>) {
        if let Some(remote) = self.remotes.get_mut(remote_name) {
            remote.set_node_name(hostname, node_name);
        }
    }

    /// Check if a host is reachable.
    pub fn host_is_reachable(&self, remote: &str, hostname: &str) -> bool {
        self.info_by_hostname(remote, hostname)
            .is_none_or(|info| info.is_reachable())
    }

    /// Get the next time to try the host and the last error if it was not reachable.
    pub fn host_time_to_next_try(
        &self,
        remote: &str,
        hostname: &str,
        current_time: i64,
    ) -> Option<(u64, String)> {
        if let Some(canary) = &self.canary_remote {
            if remote == canary {
                return None;
            }
        }
        self.info_by_hostname(remote, hostname)
            .and_then(|info| info.back_off.as_ref())
            .map(|back_off| {
                (
                    back_off.time_to_next_try(current_time),
                    back_off.last_error(),
                )
            })
    }

    // resets the back-off state of all hosts of all remotes. Used when a remote comes online again
    // when none were reachable before
    fn reset_all_back_off_states(&mut self) {
        self.canary_remote = None;

        for remote in self.remotes.values_mut() {
            remote.reset_back_off();
        }
    }

    // checks if a canary is needed: If none is set and all remotes are unreachable
    fn canary_is_needed(&mut self) -> bool {
        if let Some(canary) = &self.canary_remote {
            if self.remotes.contains_key(canary) {
                return false;
            }

            // the canary remote vanished from the cache, probably was de-configured
            self.canary_remote = None;
        }

        for remote in self.remotes.values() {
            if remote.is_reachable() {
                return false;
            }
        }
        true
    }

    /// Get the next time to try the remote and the last error if it was not reachable.
    pub fn remote_time_to_next_try(
        &self,
        remote: &str,
        current_time: i64,
    ) -> Option<(u64, String)> {
        // We're the designated canary remote, so pretend we don't have back-off state
        if let Some(canary) = &self.canary_remote {
            if canary == remote {
                return None;
            }
        }

        match self.remotes.get(remote) {
            Some(remote) => {
                let mut time = u64::MAX;
                let mut err = String::new();
                for info in remote.hosts.values() {
                    if let Some(back_off) = &info.back_off {
                        let node_time = back_off.time_to_next_try(current_time);
                        // use the least time from the hosts
                        if node_time < time {
                            time = node_time;
                            err = back_off.last_error();
                        }
                    } else {
                        // we found a node that is reachable, return immediately
                        return None;
                    }
                }

                if time == u64::MAX {
                    // we had no node information so we're allowed to try
                    return None;
                }

                Some((time, err))
            }
            None => None, // no info about remote, are we allowed to try
        }
    }
}

/// If a remote is reachable or not
pub enum ConnectionState {
    /// The host/remote/etc. is reachable
    Reachable,
    /// The remote/host/etc. is not reachable. Contains the error.
    Unreachable(String),
}

/// An entry for a remote in a [`RemoteMappingCache`].
#[derive(Clone, Deserialize, Serialize)]
pub struct RemoteMapping {
    /// The remote type.
    pub ty: RemoteType,

    /// Maps a `hostname` to information we keep about it.
    pub hosts: HashMap<String, HostInfo>,

    /// Maps a node name to a hostname, for where we have that info.
    pub node_to_host: HashMap<String, String>,
}

impl RemoteMapping {
    pub fn new(ty: RemoteType) -> Self {
        Self {
            ty,
            hosts: HashMap::new(),
            node_to_host: HashMap::new(),
        }
    }

    /// Update the node name for a host, if the host exists (otherwise this does nothing).
    pub fn set_node_name(&mut self, hostname: &str, node_name: Option<String>) {
        if let Some(info) = self.hosts.get_mut(hostname) {
            if let Some(old) = info.node_name.take() {
                self.node_to_host.remove(&old);
            }
            info.node_name = node_name;
            if let Some(new) = &info.node_name {
                self.node_to_host.insert(new.clone(), hostname.to_string());
            }
        }
    }

    fn is_reachable(&self) -> bool {
        if self.hosts.is_empty() {
            return true;
        }
        for host in self.hosts.values() {
            if host.is_reachable() {
                return true;
            }
        }
        false
    }

    fn reset_back_off(&mut self) {
        for host in self.hosts.values_mut() {
            host.set_reachable(ConnectionState::Reachable);
        }
    }
}

/// All the data we keep cached for nodes found in [`RemoteMapping`].
#[derive(Clone, Deserialize, Serialize)]
pub struct HostInfo {
    /// This is the host name associated with this node.
    pub hostname: String,

    /// This is the cluster side node name, if we know it.
    node_name: Option<String>,

    /// Per host back off config
    #[serde(default, skip_serializing_if = "Option::is_none")]
    back_off: Option<BackOffState>,
}

impl HostInfo {
    pub fn new(hostname: String) -> Self {
        Self {
            hostname,
            node_name: None,
            back_off: None,
        }
    }

    pub fn node_name(&self) -> Option<&str> {
        self.node_name.as_deref()
    }

    /// Sets the host's reachable status.
    /// Returns the next timestamp when it's allowed to retry if set.
    pub fn set_reachable(&mut self, connection_state: ConnectionState) -> Option<i64> {
        match connection_state {
            ConnectionState::Reachable => {
                self.back_off = None;
            }
            ConnectionState::Unreachable(err) => {
                let time = epoch_i64();
                match &mut self.back_off {
                    Some(back_off) => return back_off.retried(time, err),
                    None => self.back_off = Some(BackOffState::new(time, err)),
                }
            }
        }
        None
    }

    pub fn is_reachable(&self) -> bool {
        self.back_off.is_none()
    }
}
