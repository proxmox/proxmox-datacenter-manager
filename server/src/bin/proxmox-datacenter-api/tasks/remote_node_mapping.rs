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

use std::future::Future;
use std::pin::pin;

use anyhow::{bail, Error};
use tokio::task::JoinHandle;

use proxmox_config_digest::ConfigDigest;
use proxmox_section_config::typed::SectionConfigData;

use pdm_api_types::remotes::{Remote, RemoteType};

use server::remote_cache::{self, RemoteMappingCache};
use server::task_utils;

const CONFIG_POLL_INTERVAL: u64 = 60;

fn spawn_aborted_on_shutdown<F>(future: F) -> JoinHandle<()>
where
    F: Future + Send + 'static,
{
    tokio::spawn(async move {
        // TODO: The wrapping in a select(shutdown_future, fut) should probably be a helper in
        // `proxmox_daemon`.
        let future = pin!(future);
        let abort_future = pin!(proxmox_daemon::shutdown_future());
        futures::future::select(future, abort_future).await;
    })
}

pub fn start_task() {
    spawn_aborted_on_shutdown(CachingTask::default().run());
}

async fn poll_interval() {
    let delay_target = task_utils::next_aligned_instant(CONFIG_POLL_INTERVAL);
    tokio::time::sleep_until(tokio::time::Instant::from_std(delay_target)).await;
}

#[derive(Default)]
struct CachingTask {
    current_name_task: Option<JoinHandle<()>>,
    last_config_digest: Option<ConfigDigest>,
}

impl CachingTask {
    /// The caching task's main entry point.
    async fn run(mut self) {
        loop {
            self.run_once().await;
            poll_interval().await;
        }
    }

    /// A single iteration of the caching task.
    #[tracing::instrument(skip_all, name = "remote_node_caching")]
    async fn run_once(&mut self) {
        let (config, digest) = match pdm_config::remotes::config() {
            Ok(cd) => cd,
            Err(err) => {
                log::error!("failed to load remote config, not updating cache - {err:?}");
                return;
            }
        };

        if self
            .last_config_digest
            .as_ref()
            .is_none_or(|d| digest != *d)
        {
            log::trace!("new config - updating remote node name cache");
            self.last_config_digest = Some(digest);

            // the config got updated - abort the current name-fetching task, we'll
            // spawn a new one
            if let Some(name_task) = self.current_name_task.take() {
                log::trace!("aborting query task");
                name_task.abort();
            }

            if let Err(err) = self.config_update(&config) {
                log::error!("error updating remote node cache: {err:?}");
            }
        }

        if self
            .current_name_task
            .as_ref()
            .is_none_or(|task| task.is_finished())
        {
            log::trace!("name task finished, starting reachability query task");
            self.current_name_task =
                Some(spawn_aborted_on_shutdown(Self::query_node_names(config)));
        }
    }

    /// If the `remotes.cfg` was updated we need to go over all the remotes and see if there is any
    /// information we need to update.
    fn config_update(&mut self, config: &SectionConfigData<Remote>) -> Result<(), Error> {
        let mut cache = RemoteMappingCache::write()?;

        // prune remotes which were removed:
        cache
            .remotes
            .retain(|name, _entry| config.contains_key(name));

        // now update the existing remotes:
        for (name, remote) in config {
            self.prune_remote_nodes(&mut cache, name, remote);
        }

        cache.save()?;

        Ok(())
    }

    fn prune_remote_nodes(&mut self, cache: &mut RemoteMappingCache, name: &str, remote: &Remote) {
        let entry = cache
            .remotes
            .entry(name.to_string())
            .or_insert_with(|| remote_cache::RemoteMapping::new(remote.ty));

        // if the entry changed type, clear it
        if entry.ty != remote.ty {
            *entry = remote_cache::RemoteMapping::new(remote.ty);
        }

        // Only PVE entries currently have a node cache, so skip non-PVE remotes:
        if remote.ty != RemoteType::Pve {
            return;
        }

        // prune nodes which were removed:
        entry.hosts.retain(|hostname, info| {
            let retain = remote.nodes.iter().any(|node| node.hostname == *hostname);
            if !retain {
                if let Some(node_name) = info.node_name() {
                    entry.node_to_host.remove(node_name);
                }
            }
            retain
        });

        // make sure currently known hostnames exist in the cache at least empty:
        for node in &remote.nodes {
            if !entry.hosts.contains_key(&node.hostname) {
                entry.hosts.insert(
                    node.hostname.clone(),
                    remote_cache::HostInfo::new(node.hostname.clone()),
                );
            }
        }
    }

    #[tracing::instrument(skip_all)]
    async fn query_node_names(config: SectionConfigData<Remote>) {
        for (_name, remote) in &config {
            log::trace!("update remote {:?}", remote.id);
            if let Err(err) = Self::query_node_names_for_remote(remote).await {
                log::error!("error updating node name cache - {err:?}");
            }
        }
    }

    async fn query_node_names_for_remote(remote: &Remote) -> Result<(), Error> {
        // Only PVE entries currently have a node cache, so skip non-PVE remotes:
        if remote.ty != RemoteType::Pve {
            return Ok(());
        }

        // now add new nodes
        for node in &remote.nodes {
            log::debug!("querying remote {:?} node {:?}", remote.id, node.hostname);

            // if the host is new, we need to query its name
            let query_result = match query_node_name(remote, &node.hostname).await {
                Ok(node_name) => Some(node_name),
                Err(err) => {
                    log::error!(
                        "failed to query info for remote '{}' node '{}' - {err:?}",
                        remote.id,
                        node.hostname
                    );
                    None
                }
            };

            let mut cache = RemoteMappingCache::write()?;
            if let Some(info) = cache.info_by_hostname_mut(&remote.id, &node.hostname) {
                info.reachable = query_result.is_some();
            }
            if let Some(node_name) = query_result {
                cache.set_node_name(&remote.id, &node.hostname, Some(node_name));
            }
            cache.save()?;
        }

        Ok(())
    }
}

/// Calls `/cluster/status` directly on a specific node to find its name.
async fn query_node_name(remote: &Remote, hostname: &str) -> Result<String, Error> {
    log::trace!("querying node name {hostname:?} for remote {:?}", remote.id);
    let client = server::connection::make_pve_client_with_endpoint(remote, Some(hostname))?;
    let node_status_list = client.cluster_status().await?;
    for node in node_status_list {
        if node.local == Some(true) {
            return Ok(node.name);
        }
    }
    bail!("failed to connect to node {hostname}");
}
