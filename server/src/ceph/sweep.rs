//! Ceph cluster auto-detection.
//!
//! Probes PVE remotes for a configured Ceph cluster, deduplicates the results
//! by fsid (several remotes can see the same cluster), and upserts
//! `ceph-clusters.cfg`. Runs as a one-shot probe when a remote is added and,
//! later, on a periodic sweep.
//!
//! Phase 1b is intentionally minimal: a probe either finds a cluster or finds
//! nothing (no Ceph, or the remote is unreachable - both treated the same). The
//! `detected`/`unreachable`/`gone` state machine, removal of vanished clusters,
//! and concurrent probing via `ParallelFetcher` are later refinements.
//!
//! Members are taken from the Ceph mon quorum (`quorum_names` in `ceph status`).
//! By PVE convention a monitor is named after the node it runs on, so the mon
//! name doubles as the PVE node name used for node-level dispatch.

use std::collections::BTreeMap;

use anyhow::Error;

use pdm_api_types::ceph::{CephCluster, CephClusterState, CephMember, CephMemberKind};
use pdm_api_types::remotes::RemoteType;
use pdm_config::ceph::CephClustersConfig;

/// What probing a single remote found.
struct Detected {
    fsid: String,
    /// Ceph member node names (the mon quorum).
    nodes: Vec<String>,
}

/// Probe one PVE remote for a configured Ceph cluster.
///
/// Returns `None` when the remote has no Ceph or is currently unreachable; the
/// two are not distinguished in this phase.
async fn probe_remote(remote_id: &str) -> Result<Option<Detected>, Error> {
    let client = crate::api::pve::connect_to_remote_by_id(remote_id)?;
    let status = match client.cluster_ceph_status().await {
        Ok(status) => status,
        Err(_) => return Ok(None),
    };

    let Some(fsid) = status.get("fsid").and_then(|v| v.as_str()) else {
        return Ok(None);
    };

    // Populate the status cache as a side effect so the cluster list has
    // health and quorum data without a separate fetch. Best-effort.
    if let Err(e) = super::cache::store_status(fsid, &status).await {
        log::warn!("failed to cache ceph status for {fsid}: {e}");
    }

    let nodes = status
        .get("quorum_names")
        .and_then(|v| v.as_array())
        .map(|names| {
            names
                .iter()
                .filter_map(|n| n.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    Ok(Some(Detected {
        fsid: fsid.to_string(),
        nodes,
    }))
}

/// Probe the given PVE remotes and upsert what was found into
/// `ceph-clusters.cfg`. Clusters carrying an operator tombstone (`forgotten`)
/// are left untouched. Suitable for both the one-shot probe on remote-add and
/// the full sweep.
pub async fn detect_and_upsert(remote_ids: &[String]) -> Result<(), Error> {
    // fsid -> (remote, node) members across all probed remotes.
    let mut found: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for remote_id in remote_ids {
        if let Some(detected) = probe_remote(remote_id).await? {
            let members = found.entry(detected.fsid).or_default();
            for node in detected.nodes {
                members.push((remote_id.clone(), node));
            }
        }
    }

    if found.is_empty() {
        return Ok(());
    }

    let _lock = pdm_config::ceph::lock_config()?;
    let (mut config, _) = pdm_config::ceph::config()?;
    let mut changed = false;
    for (fsid, members) in &found {
        changed |= upsert_cluster(&mut config, fsid, members);
    }
    if changed {
        pdm_config::ceph::save_config(&config)?;
    }
    Ok(())
}

/// Probe every PVE remote for Ceph and upsert the registry.
pub async fn sweep() -> Result<(), Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let pve_remotes: Vec<String> = remotes
        .into_iter()
        .filter(|(_, remote)| remote.ty == RemoteType::Pve)
        .map(|(id, _)| id)
        .collect();
    detect_and_upsert(&pve_remotes).await
}

/// Clean up the registry after a PVE remote was removed: drop its Ceph members
/// and any cluster that is left without members.
pub fn on_remote_removed(remote_id: &str) -> Result<(), Error> {
    let _lock = pdm_config::ceph::lock_config()?;
    let (mut config, _) = pdm_config::ceph::config()?;
    let dropped_members = config.drop_pve_members_for_remote(remote_id);
    let dropped_clusters = config.drop_clusters_without_members();
    if dropped_members || !dropped_clusters.is_empty() {
        pdm_config::ceph::save_config(&config)?;
    }
    Ok(())
}

/// Upsert one detected cluster and its members. Returns whether anything
/// changed. A tombstoned (`forgotten`) cluster is left untouched.
fn upsert_cluster(
    config: &mut CephClustersConfig,
    fsid: &str,
    members: &[(String, String)],
) -> bool {
    if config
        .clusters
        .get(fsid)
        .is_some_and(|c| c.forgotten.is_some())
    {
        return false;
    }

    let mut changed = false;
    {
        let cluster = config
            .clusters
            .entry(fsid.to_string())
            .or_insert_with(|| {
                changed = true;
                CephCluster {
                    id: fsid.to_string(),
                    display_name: None,
                    public_network: None,
                    cluster_network: None,
                    stretch_mode: None,
                    tiebreaker_mon: None,
                    state: None,
                    last_seen_missing: None,
                    forgotten: None,
                }
            });
        if cluster.state != Some(CephClusterState::Detected) || cluster.last_seen_missing.is_some()
        {
            cluster.state = Some(CephClusterState::Detected);
            cluster.last_seen_missing = None;
            changed = true;
        }
    }

    for (remote, node) in members {
        let id = format!("{fsid}-{node}");
        if !config.members.contains_key(&id) {
            config.members.insert(
                id.clone(),
                CephMember {
                    id,
                    cluster: fsid.to_string(),
                    kind: CephMemberKind::Pve,
                    remote: Some(remote.clone()),
                    node: Some(node.clone()),
                    address: None,
                    fingerprint: None,
                    authid: None,
                    site: None,
                },
            );
            changed = true;
        }
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    const FSID: &str = "4b4f7f8e-1234-5678-abcd-ef0123456789";

    fn members() -> Vec<(String, String)> {
        vec![
            ("pve-east".to_string(), "node1".to_string()),
            ("pve-east".to_string(), "node2".to_string()),
        ]
    }

    #[test]
    fn upsert_creates_then_is_idempotent() {
        let mut cfg = CephClustersConfig::default();
        assert!(upsert_cluster(&mut cfg, FSID, &members()));
        assert_eq!(cfg.clusters.len(), 1);
        assert_eq!(cfg.members.len(), 2);
        assert_eq!(
            cfg.clusters[FSID].state,
            Some(CephClusterState::Detected)
        );
        // Second run with the same input changes nothing.
        assert!(!upsert_cluster(&mut cfg, FSID, &members()));
        assert_eq!(cfg.members.len(), 2);
    }

    #[test]
    fn upsert_respects_tombstone() {
        let mut cfg = CephClustersConfig::default();
        upsert_cluster(&mut cfg, FSID, &members());
        cfg.clusters.get_mut(FSID).unwrap().forgotten = Some(1_700_000_000);
        cfg.members.clear();

        // A forgotten cluster is not re-populated.
        assert!(!upsert_cluster(&mut cfg, FSID, &members()));
        assert!(cfg.members.is_empty());
    }

    #[test]
    fn upsert_refreshes_state_after_gone() {
        let mut cfg = CephClustersConfig::default();
        upsert_cluster(&mut cfg, FSID, &members());
        let cluster = cfg.clusters.get_mut(FSID).unwrap();
        cluster.state = Some(CephClusterState::Gone);
        cluster.last_seen_missing = Some(1_700_000_000);

        assert!(upsert_cluster(&mut cfg, FSID, &members()));
        assert_eq!(cfg.clusters[FSID].state, Some(CephClusterState::Detected));
        assert_eq!(cfg.clusters[FSID].last_seen_missing, None);
    }
}
