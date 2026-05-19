use anyhow::{bail, Error};
use serde::{Deserialize, Serialize};

use proxmox_apt_api_types::{APTRepositoriesResult, APTStandardRepoSummary, APTUpdateInfo};

use pdm_api_types::remote_updates::{
    NodeUpdateStatus, NodeUpdateSummary, NodeUpdateSummaryWrapper, PackageVersion,
    ProductRepositoryStatus, RemoteUpdateStatus, RemoteUpdateSummary, UpdateSummary,
};
use pdm_api_types::remotes::{Remote, RemoteType};
use pdm_api_types::RemoteUpid;

use crate::namespaced_cache::CacheError;
use crate::parallel_fetcher::ParallelFetcher;
use crate::{api_cache, connection};

const OLD_CACHEFILE: &str = concat!(pdm_buildcfg::PDM_CACHE_DIR_M!(), "/remote-updates.json");

const UPDATE_SUMMARY_CACHE_KEY: &str = "remote-updates";

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
struct NodeUpdateInfo {
    updates: Vec<APTUpdateInfo>,
    last_refresh: i64,
    versions: Vec<PackageVersion>,
    repository_status: ProductRepositoryStatus,
}

impl From<&NodeUpdateInfo> for NodeUpdateSummary {
    fn from(value: &NodeUpdateInfo) -> Self {
        Self {
            number_of_updates: value.updates.len() as u32,
            last_refresh: value.last_refresh,
            status: NodeUpdateStatus::Success,
            status_message: None,
            versions: value.versions.clone(),
            repository_status: value.repository_status,
        }
    }
}

/// Return a list of available updates for a given remote node.
pub async fn list_available_updates(
    remote: Remote,
    node: String,
) -> Result<Vec<APTUpdateInfo>, Error> {
    let updates = fetch_available_updates((), remote.clone(), node.clone()).await?;

    let summary = NodeUpdateSummary::from(&updates);

    // Update cache entry asynchronously, no need to wait for it.
    tokio::task::spawn(async move {
        if let Err(err) = update_cached_summary_for_node(remote, node, summary).await {
            log::error!("could not update 'remote-updates' API cache entry: {err}");
        }
    });

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
pub async fn get_available_updates_summary() -> Result<UpdateSummary, Error> {
    let (config, _digest) = pdm_config::remotes::config()?;

    let cache_content = get_cached_summary_or_default().await?;

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

/// Return cached update information from specific remote
pub async fn get_available_updates_for_remote(remote: &str) -> Result<RemoteUpdateSummary, Error> {
    let (config, _digest) = pdm_config::remotes::config()?;

    if let Some(remote) = config.get(remote) {
        let cache_content = get_cached_summary_or_default().await?;
        Ok(cache_content
            .remotes
            .get(&remote.id)
            .cloned()
            .unwrap_or_else(|| RemoteUpdateSummary {
                nodes: NodeUpdateSummaryWrapper::default(),
                remote_type: remote.ty,
                status: RemoteUpdateStatus::Unknown,
            }))
    } else {
        bail!("no such remote '{remote}'");
    }
}

/// Turn a cache-read result into a summary, falling back to the default on any error.
///
/// This ensures that the cache self-heals if an entry got corrupted for some reason,
/// instead of leaving the surrounding code path stuck on a read error it has no way to
/// recover from.
fn summary_or_default(result: Result<Option<UpdateSummary>, CacheError>) -> UpdateSummary {
    result
        .inspect_err(|err| {
            log::error!("could not read 'remote-updates' entry from API cache: {err}")
        })
        .unwrap_or_default()
        .unwrap_or_default()
}

/// Read the cached summary from the API cache, or return a default, empty summary.
async fn get_cached_summary_or_default() -> Result<UpdateSummary, Error> {
    let guard = api_cache::read_global().await?;
    let summary = summary_or_default(guard.get::<UpdateSummary>(UPDATE_SUMMARY_CACHE_KEY).await);
    Ok(summary)
}

async fn update_cached_summary_for_node(
    remote: Remote,
    node: String,
    node_data: NodeUpdateSummary,
) -> Result<(), Error> {
    let cache = api_cache::write_global().await?;
    let mut cache_content =
        summary_or_default(cache.get::<UpdateSummary>(UPDATE_SUMMARY_CACHE_KEY).await);

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
    cache.set(UPDATE_SUMMARY_CACHE_KEY, cache_content).await?;

    Ok(())
}

/// Refresh the remote update cache.
pub async fn refresh_update_summary_cache(remotes: Vec<Remote>) -> Result<(), Error> {
    let fetcher = ParallelFetcher::new(());

    let fetch_response = fetcher
        .do_for_all_remote_nodes(remotes.clone().into_iter(), fetch_available_updates)
        .await;

    let cache = api_cache::write_global().await?;
    let mut content =
        summary_or_default(cache.get::<UpdateSummary>(UPDATE_SUMMARY_CACHE_KEY).await);

    // Clean out any remotes that might have been removed from the remote config in the meanwhile.
    content
        .remotes
        .retain(|remote, _| fetch_response.iter().any(|r| r.remote() == remote));

    for remote_response in fetch_response {
        let remote_name = remote_response.remote().to_string();

        let entry = content
            .remotes
            .entry(remote_name.clone())
            .or_insert_with(|| RemoteUpdateSummary {
                nodes: Default::default(),
                remote_type: remote_response.remote_type(),
                status: RemoteUpdateStatus::Success,
            });

        match remote_response.nodes() {
            Ok(node_responses) => {
                // Clean out any nodes that might have been removed from the cluster in the meanwhile.
                entry
                    .nodes
                    .retain(|name, _| node_responses.iter().any(|n| n.node_name() == name));

                entry.status = RemoteUpdateStatus::Success;

                for node_response in node_responses {
                    let node_name = node_response.node_name().to_string();

                    match node_response.data() {
                        Ok(update_info) => {
                            entry.nodes.insert(node_name, update_info.into());
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

    cache.set(UPDATE_SUMMARY_CACHE_KEY, content).await?;

    cleanup_old_cachefile().await?;

    Ok(())
}

// FIXME: We can remove this pretty soon.
async fn cleanup_old_cachefile() -> Result<(), Error> {
    tokio::task::spawn_blocking(|| {
        if let Err(err) = std::fs::remove_file(OLD_CACHEFILE) {
            if err.kind() != std::io::ErrorKind::NotFound {
                log::error!(
                    "could not clean up old remote update cache file {OLD_CACHEFILE}: {err}"
                );
            }
        } else {
            log::info!("removed obsolete remote update cachefile {OLD_CACHEFILE}")
        }
    })
    .await?;

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

    let summary = APTStandardRepoSummary::from_repos(&config.standard_repos);
    for handle in &summary.unrecognized {
        log::warn!("encountered unknown APT repository handle variant '{handle}'");
    }

    if !(summary.has_enterprise || summary.has_no_subscription || summary.has_test) {
        return ProductRepositoryStatus::NoProductRepository;
    }
    if (summary.has_enterprise || summary.has_ceph_enterprise) && !active_subscription {
        return ProductRepositoryStatus::MissingSubscriptionForEnterprise;
    }
    if summary.has_test
        || summary.has_no_subscription
        || summary.has_ceph_no_subscription
        || summary.has_ceph_test
    {
        return ProductRepositoryStatus::NonProductionReady;
    }
    ProductRepositoryStatus::Ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use proxmox_apt_api_types::APTStandardRepository;

    use ProductRepositoryStatus::{
        MissingSubscriptionForEnterprise, NoProductRepository, NonProductionReady,
    };

    fn repos_result(enabled: &[&str]) -> APTRepositoriesResult {
        APTRepositoriesResult {
            files: Vec::new(),
            digest: [0u8; 32].into(),
            infos: Vec::new(),
            errors: Vec::new(),
            standard_repos: enabled
                .iter()
                .map(|h| APTStandardRepository {
                    handle: h.parse().expect("test wire string must parse"),
                    status: Some(true),
                    name: String::new(),
                    description: String::new(),
                })
                .collect(),
        }
    }

    // Ceph variant used by the test rows below. Picked once so a release rotation
    // (Squid EOL, Tentacle EOL, Umbrella added, ...) only needs a single line change here.
    // api-types' `is_ceph_release` drift guard pins the per-variant bucketing already, so PDM
    // tests do not need to enumerate Ceph variants themselves.
    const CEPH_REL: &str = "ceph-tentacle";

    /// Rows here are limited to the cases not covered by the function structure itself.
    /// Trivial happy-path / single-field assertions just restate the if-statements and are
    /// omitted; api-types' `standard_repo_summary_buckets` already pins the summary helper.
    #[test]
    fn check_repository_status_arms() {
        let ceph_ent = format!("{CEPH_REL}-enterprise");
        let ceph_nosub = format!("{CEPH_REL}-no-subscription");
        for (label, enabled, sub, want) in [
            // (label, enabled handles, has-subscription, expected status)

            // The `has_enterprise || has_ceph_enterprise` OR: Ceph alone must still demand a sub.
            (
                "ceph enterprise without product sub",
                vec!["enterprise", &ceph_ent],
                false,
                MissingSubscriptionForEnterprise,
            ),
            // Cross-channel interaction: a Ceph no-subscription line dampens an otherwise-okay
            // product to NonProductionReady even with an active subscription.
            (
                "enterprise dampened by ceph no-sub",
                vec!["enterprise", &ceph_nosub],
                true,
                NonProductionReady,
            ),
            // Pin pre-existing limitations: a ceph-only / unknown-only host degrades to
            // NoProductRepository because the "has any product repo" predicate ignores both
            // has_ceph_* and summary.unrecognized. The planned follow-up will surface a
            // dedicated Unknown status; until then these rows make any silent change loud.
            ("ceph-only host", vec![&ceph_ent], true, NoProductRepository),
            ("unrecognized only", vec!["future-channel"], true, NoProductRepository),
        ] {
            let r = repos_result(&enabled);
            assert_eq!(check_repository_status(&r, sub), want, "{label}");
        }
    }
}
