use std::fs::File;
use std::io::ErrorKind;

use anyhow::Error;
use serde::{Deserialize, Serialize};

use proxmox_apt_api_types::{APTRepositoriesResult, APTRepositoryHandle, APTUpdateInfo};

use pdm_api_types::remote_updates::{
    NodeUpdateStatus, NodeUpdateSummary, NodeUpdateSummaryWrapper, PackageVersion,
    ProductRepositoryStatus, RemoteUpdateStatus, RemoteUpdateSummary, UpdateSummary,
};
use pdm_api_types::remotes::{Remote, RemoteType};
use pdm_api_types::RemoteUpid;
use pdm_buildcfg::PDM_CACHE_DIR_M;

use crate::connection;
use crate::parallel_fetcher::{NodeResults, ParallelFetcher};

pub const UPDATE_CACHE: &str = concat!(PDM_CACHE_DIR_M!(), "/remote-updates.json");

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
struct NodeUpdateInfo {
    updates: Vec<APTUpdateInfo>,
    last_refresh: i64,
    versions: Vec<PackageVersion>,
    repository_status: ProductRepositoryStatus,
}

impl From<NodeUpdateInfo> for NodeUpdateSummary {
    fn from(value: NodeUpdateInfo) -> Self {
        Self {
            number_of_updates: value.updates.len() as u32,
            last_refresh: value.last_refresh,
            status: NodeUpdateStatus::Success,
            status_message: None,
            versions: value.versions,
            repository_status: value.repository_status,
        }
    }
}

/// Return a list of available updates for a given remote node.
pub async fn list_available_updates(
    remote: Remote,
    node: &str,
) -> Result<Vec<APTUpdateInfo>, Error> {
    let updates = fetch_available_updates((), remote.clone(), node.to_string()).await?;

    update_cached_summary_for_node(remote, node.into(), updates.clone().into()).await?;

    Ok(updates.updates)
}

/// Trigger `apt update` on a remote node.
///
/// The function returns a `[RemoteUpid]` for the started update task.
pub async fn update_apt_database(remote: &Remote, node: &str) -> Result<RemoteUpid, Error> {
    match remote.ty {
        RemoteType::Pve => {
            let client = connection::make_pve_client(remote)?;

            let params = pve_api_types::AptUpdateParams {
                notify: Some(false),
                quiet: Some(false),
            };
            let upid = client.update_apt_database(node, params).await?;

            crate::api::pve::new_remote_upid(remote.id.clone(), upid).await
        }
        RemoteType::Pbs => {
            let client = connection::make_pbs_client(remote)?;

            let params = crate::pbs_client::AptUpdateParams {
                notify: Some(false),
                quiet: Some(false),
            };
            let upid = client.update_apt_database(params).await?;

            crate::api::pbs::new_remote_upid(remote.id.clone(), upid).await
        }
    }
}

/// Get the changelog for a given package.
pub async fn get_changelog(remote: &Remote, node: &str, package: String) -> Result<String, Error> {
    match remote.ty {
        RemoteType::Pve => {
            let client = connection::make_pve_client(remote)?;

            client
                .get_package_changelog(node, package, None)
                .await
                .map_err(Into::into)
        }
        RemoteType::Pbs => {
            let client = connection::make_pbs_client(remote)?;

            client
                .get_package_changelog(package, None)
                .await
                .map_err(Into::into)
        }
    }
}

/// Get update summary for all managed remotes.
pub fn get_available_updates_summary() -> Result<UpdateSummary, Error> {
    let (config, _digest) = pdm_config::remotes::config()?;

    let cache_content = get_cached_summary_or_default()?;

    let mut summary = UpdateSummary::default();

    for (remote_name, remote) in &config {
        match cache_content.remotes.get(remote_name) {
            Some(remote_summary) => {
                summary
                    .remotes
                    .insert(remote_name.into(), remote_summary.clone());
            }
            None => {
                summary.remotes.insert(
                    remote_name.into(),
                    RemoteUpdateSummary {
                        nodes: NodeUpdateSummaryWrapper::default(),
                        remote_type: remote.ty,
                        status: RemoteUpdateStatus::Unknown,
                    },
                );
            }
        }
    }

    Ok(summary)
}

fn get_cached_summary_or_default() -> Result<UpdateSummary, Error> {
    match File::open(UPDATE_CACHE) {
        Ok(file) => {
            let content = match serde_json::from_reader(file) {
                Ok(cache_content) => cache_content,
                Err(err) => {
                    log::error!("failed to deserialize remote update cache: {err:#}");
                    Default::default()
                }
            };

            Ok(content)
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(Default::default()),
        Err(err) => Err(err.into()),
    }
}

async fn update_cached_summary_for_node(
    remote: Remote,
    node: String,
    node_data: NodeUpdateSummary,
) -> Result<(), Error> {
    let mut file = File::open(UPDATE_CACHE)?;
    let mut cache_content: UpdateSummary = serde_json::from_reader(&mut file)?;
    let remote_entry =
        cache_content
            .remotes
            .entry(remote.id)
            .or_insert_with(|| RemoteUpdateSummary {
                nodes: Default::default(),
                remote_type: remote.ty,
                status: RemoteUpdateStatus::Success,
            });

    remote_entry.nodes.insert(node, node_data);

    let options = proxmox_product_config::default_create_options();
    proxmox_sys::fs::replace_file(
        UPDATE_CACHE,
        &serde_json::to_vec(&cache_content)?,
        options,
        true,
    )?;

    Ok(())
}

/// Refresh the remote update cache.
pub async fn refresh_update_summary_cache(remotes: Vec<Remote>) -> Result<(), Error> {
    let fetcher = ParallelFetcher::new(());

    let fetch_results = fetcher
        .do_for_all_remote_nodes(remotes.clone().into_iter(), fetch_available_updates)
        .await;

    let mut content = get_cached_summary_or_default()?;

    for (remote_name, result) in fetch_results.remote_results {
        let entry = content
            .remotes
            .entry(remote_name.clone())
            .or_insert_with(|| {
                // unwrap: remote name came from the same config, should be safe.
                // TODO: Include type in ParallelFetcher results - should be much more efficient.
                let remote_type = remotes.iter().find(|r| r.id == remote_name).unwrap().ty;

                RemoteUpdateSummary {
                    nodes: Default::default(),
                    remote_type,
                    status: RemoteUpdateStatus::Success,
                }
            });

        match result {
            Ok(remote_result) => {
                entry.status = RemoteUpdateStatus::Success;

                for (node_name, node_result) in remote_result.node_results {
                    match node_result {
                        Ok(NodeResults { data, .. }) => {
                            entry.nodes.insert(node_name, data.into());
                        }
                        Err(err) => {
                            // Could not fetch updates from node
                            entry.nodes.insert(
                                node_name.clone(),
                                NodeUpdateSummary {
                                    number_of_updates: 0,
                                    last_refresh: 0,
                                    status: NodeUpdateStatus::Error,
                                    status_message: Some(format!("{err:#}")),
                                    versions: Vec::new(),
                                    repository_status: ProductRepositoryStatus::Error,
                                },
                            );
                            log::error!(
                                "could not fetch available updates from node '{node_name}': {err}"
                            );
                        }
                    }
                }
            }
            Err(err) => {
                entry.status = RemoteUpdateStatus::Error;
                entry.nodes.clear();
                log::error!("could not fetch available updates from remote '{remote_name}': {err}");
            }
        }
    }

    let options = proxmox_product_config::default_create_options();
    proxmox_sys::fs::replace_file(UPDATE_CACHE, &serde_json::to_vec(&content)?, options, true)?;

    Ok(())
}

async fn fetch_available_updates(
    _context: (),
    remote: Remote,
    node: String,
) -> Result<NodeUpdateInfo, Error> {
    match remote.ty {
        RemoteType::Pve => {
            let client = connection::make_pve_client(&remote)?;

            let updates = client
                .list_available_updates(&node)
                .await?
                .into_iter()
                .map(map_pve_update_info)
                .collect();

            let versions = client.get_package_versions(&node).await?;
            let versions = versions
                .into_iter()
                .filter(|v| v.package == "pve-manager")
                .map(map_pve_package_version)
                .collect();

            let repos = client.get_apt_repositories(&node).await?;
            let subscription_info = client.get_subscription(&node).await?;

            let has_active_subscription =
                subscription_info.status == pve_api_types::NodeSubscriptionInfoStatus::Active;

            let repository_status = check_repository_status(&repos, has_active_subscription);

            Ok(NodeUpdateInfo {
                last_refresh: proxmox_time::epoch_i64(),
                updates,
                versions,
                repository_status,
            })
        }
        RemoteType::Pbs => {
            let client = connection::make_pbs_client(&remote)?;
            let updates = client.list_available_updates().await?;

            let versions = client.get_package_versions().await?;
            let versions = versions
                .into_iter()
                .filter(|v| v.package == "proxmox-backup-server")
                .map(map_pbs_package_version)
                .collect();

            let repos = client.get_apt_repositories().await?;
            let subscription_info = client.get_subscription().await?;

            let has_active_subscription =
                subscription_info.status == proxmox_subscription::SubscriptionStatus::Active;

            let repository_status = check_repository_status(&repos, has_active_subscription);

            Ok(NodeUpdateInfo {
                last_refresh: proxmox_time::epoch_i64(),
                updates,
                versions,
                repository_status,
            })
        }
    }
}

fn map_pve_update_info(info: pve_api_types::AptUpdateInfo) -> APTUpdateInfo {
    APTUpdateInfo {
        package: info.package,
        title: info.title,
        arch: info.arch.to_string(),
        description: info.description,
        version: info.version,
        old_version: info.old_version,
        origin: info.origin,
        priority: info.priority,
        section: info.section,
        extra_info: None,
    }
}

fn map_pve_package_version(info: pve_api_types::InstalledPackage) -> PackageVersion {
    PackageVersion {
        package: info.package,
        version: info.old_version.unwrap_or_default(),
    }
}

fn map_pbs_package_version(info: pbs_api_types::APTUpdateInfo) -> PackageVersion {
    PackageVersion {
        package: info.package,
        version: info.old_version.unwrap_or_default(),
    }
}

fn check_repository_status(
    config: &APTRepositoriesResult,
    active_subscription: bool,
) -> ProductRepositoryStatus {
    if !config.errors.is_empty() {
        return ProductRepositoryStatus::Error;
    }

    let mut has_enterprise = false;
    let mut has_no_subscription = false;
    let mut has_test = false;
    let mut has_ceph_enterprise = false;
    let mut has_ceph_no_subscription = false;
    let mut has_ceph_test = false;

    for repo in &config.standard_repos {
        if repo.status != Some(true) {
            continue;
        }
        match repo.handle {
            APTRepositoryHandle::CephSquidEnterprise => has_ceph_enterprise = true,
            APTRepositoryHandle::CephSquidNoSubscription => has_ceph_no_subscription = true,
            APTRepositoryHandle::CephSquidTest => has_ceph_test = true,
            APTRepositoryHandle::Enterprise => has_enterprise = true,
            APTRepositoryHandle::NoSubscription => has_no_subscription = true,
            APTRepositoryHandle::Test => has_test = true,
            APTRepositoryHandle::UnknownEnumValue(s) => {
                log::warn!("encountered unkown APT repository handle variant {s}");
            }
        }
    }

    if !(has_enterprise | has_no_subscription | has_test) {
        return ProductRepositoryStatus::NoProductRepository;
    }

    if has_enterprise && !active_subscription {
        return ProductRepositoryStatus::MissingSubscriptionForEnterprise;
    }

    if has_ceph_enterprise && !active_subscription {
        return ProductRepositoryStatus::MissingSubscriptionForEnterprise;
    }

    if has_test || has_no_subscription {
        return ProductRepositoryStatus::NonProductionReady;
    }

    if has_ceph_no_subscription || has_ceph_test {
        return ProductRepositoryStatus::NonProductionReady;
    }

    ProductRepositoryStatus::Ok
}
