//! Ceph cluster auto-detection.
//!
//! Probes PVE remotes for a configured Ceph cluster, deduplicates the results
//! by fsid (several remotes can see the same cluster), and upserts
//! `ceph-clusters.cfg`. Runs as a one-shot probe when a remote is added and,
//! later, on a periodic sweep.
//!
//! A probe classifies each remote three ways: it `Found` a cluster (an fsid is
//! present), the remote is `Reachable` but reports no Ceph, or it is
//! `Unreachable` (the request never reached the server). A found cluster is
//! upserted as `Detected`. The full [`sweep`] additionally reconciles clusters
//! it did *not* find this cycle, using the outcomes of the remotes backing
//! their members: all reachable but none reporting the fsid means `Gone` (the
//! cluster was removed); any member-remote unreachable means `Unreachable` (a
//! transient connection issue, not a removal). The one-shot [`detect_and_upsert`]
//! used on remote-add only upserts - it lacks the full-fleet coverage needed to
//! safely demote a cluster. A tombstoned (`forgotten`) cluster is never touched.
//!
//! Members are taken from the Ceph mon quorum (`quorum_names` in `ceph status`).
//! By PVE convention a monitor is named after the node it runs on, so the mon
//! name doubles as the PVE node name used for node-level dispatch.
//!
//! Concurrent probing via `ParallelFetcher` is a later refinement.

use std::collections::BTreeMap;

use anyhow::Error;

use pdm_api_types::ceph::{CephCluster, CephClusterState, CephMember, CephMemberKind};
use pdm_api_types::remotes::RemoteType;
use pdm_config::ceph::CephClustersConfig;

/// The result of probing a single PVE remote for Ceph.
enum ProbeOutcome {
    /// A Ceph cluster is configured: its fsid and mon-quorum node names.
    Found { fsid: String, nodes: Vec<String> },
    /// The remote answered but reports no Ceph cluster.
    Reachable,
    /// The request never reached the remote (connection/transport failure).
    Unreachable,
}

/// Probe one PVE remote for a configured Ceph cluster.
///
/// Infallible by design: a connection failure is a normal outcome
/// ([`ProbeOutcome::Unreachable`]), not an error that should abort the sweep.
async fn probe_remote(remote_id: &str) -> ProbeOutcome {
    let client = match crate::api::pve::connect_to_remote_by_id(remote_id) {
        Ok(client) => client,
        // Cannot even build a client (e.g. the remote config is broken); treat
        // it as unreachable rather than failing the whole sweep.
        Err(_) => return ProbeOutcome::Unreachable,
    };

    let status = match client.cluster_ceph_status().await {
        Ok(status) => status,
        // An API-level error means we reached the server (it answered, e.g.
        // "ceph not configured"); only a transport failure is unreachable.
        Err(proxmox_client::Error::Api(..))
        | Err(proxmox_client::Error::Authentication(_))
        | Err(proxmox_client::Error::Ticket(_))
        | Err(proxmox_client::Error::TfaRequired(_)) => return ProbeOutcome::Reachable,
        Err(_) => return ProbeOutcome::Unreachable,
    };

    let Some(fsid) = status.get("fsid").and_then(|v| v.as_str()) else {
        return ProbeOutcome::Reachable;
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

    ProbeOutcome::Found {
        fsid: fsid.to_string(),
        nodes,
    }
}

/// Probe the given remotes, returning the per-remote outcome and the found
/// clusters (fsid -> the `(remote, node)` members seen this cycle).
async fn probe_all(
    remote_ids: &[String],
) -> (
    BTreeMap<String, ProbeOutcome>,
    BTreeMap<String, Vec<(String, String)>>,
) {
    let mut outcomes = BTreeMap::new();
    let mut found: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for remote_id in remote_ids {
        let outcome = probe_remote(remote_id).await;
        if let ProbeOutcome::Found { fsid, nodes } = &outcome {
            let members = found.entry(fsid.clone()).or_default();
            for node in nodes {
                members.push((remote_id.clone(), node.clone()));
            }
        }
        outcomes.insert(remote_id.clone(), outcome);
    }
    (outcomes, found)
}

/// Probe the given PVE remotes and upsert what was found into
/// `ceph-clusters.cfg`. Clusters carrying an operator tombstone (`forgotten`)
/// are left untouched.
///
/// Upsert-only: this is the one-shot probe fired on remote-add, which sees a
/// single remote and therefore cannot tell a removed cluster from one whose
/// other members live on remotes it did not probe. State demotion is the full
/// [`sweep`]'s job.
pub async fn detect_and_upsert(remote_ids: &[String]) -> Result<(), Error> {
    let (_outcomes, found) = probe_all(remote_ids).await;

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

/// Probe every PVE remote for Ceph, upsert what was found, and reconcile the
/// state of registered clusters that were not found this cycle.
pub async fn sweep() -> Result<(), Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let pve_remotes: Vec<String> = remotes
        .into_iter()
        .filter(|(_, remote)| remote.ty == RemoteType::Pve)
        .map(|(id, _)| id)
        .collect();

    let (outcomes, found) = probe_all(&pve_remotes).await;

    let _lock = pdm_config::ceph::lock_config()?;
    let (mut config, _) = pdm_config::ceph::config()?;
    let mut changed = false;
    for (fsid, members) in &found {
        changed |= upsert_cluster(&mut config, fsid, members);
    }
    changed |= reconcile_unfound(&mut config, &found, &outcomes, proxmox_time::epoch_i64());
    if changed {
        pdm_config::ceph::save_config(&config)?;
    }
    Ok(())
}

/// Demote registered clusters that the sweep did not find this cycle, based on
/// the probe outcomes of the remotes backing their members.
///
/// A cluster goes `Gone` (stamping `last_seen_missing`) when every probed
/// member-remote was reachable yet none reported its fsid - it was removed. It
/// goes `Unreachable` when at least one member-remote could not be reached - a
/// transient issue, so `last_seen_missing` is left alone. A cluster with no
/// probed member-remote this cycle (no coverage) and a tombstoned cluster are
/// both left untouched.
fn reconcile_unfound(
    config: &mut CephClustersConfig,
    found: &BTreeMap<String, Vec<(String, String)>>,
    outcomes: &BTreeMap<String, ProbeOutcome>,
    now: i64,
) -> bool {
    // cluster fsid -> the remote ids of its PVE members.
    let mut member_remotes: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for member in config.members.values() {
        if member.kind == CephMemberKind::Pve {
            if let Some(remote) = member.remote.as_deref() {
                member_remotes
                    .entry(member.cluster.as_str())
                    .or_default()
                    .push(remote);
            }
        }
    }

    // Decide the new state under an immutable borrow, then apply.
    let mut updates: Vec<(String, CephClusterState)> = Vec::new();
    for (fsid, cluster) in &config.clusters {
        if found.contains_key(fsid) || cluster.forgotten.is_some() {
            continue;
        }
        let Some(remotes) = member_remotes.get(fsid.as_str()) else {
            continue;
        };
        let probed: Vec<&ProbeOutcome> = remotes.iter().filter_map(|r| outcomes.get(*r)).collect();
        if probed.is_empty() {
            continue; // no coverage for this cluster this cycle
        }
        let any_unreachable = probed
            .iter()
            .any(|o| matches!(o, ProbeOutcome::Unreachable));
        updates.push((
            fsid.clone(),
            if any_unreachable {
                CephClusterState::Unreachable
            } else {
                CephClusterState::Gone
            },
        ));
    }

    let mut changed = false;
    for (fsid, new_state) in updates {
        let cluster = config.clusters.get_mut(&fsid).unwrap();
        match new_state {
            CephClusterState::Gone => {
                if cluster.state != Some(CephClusterState::Gone) {
                    cluster.state = Some(CephClusterState::Gone);
                    changed = true;
                }
                if cluster.last_seen_missing.is_none() {
                    cluster.last_seen_missing = Some(now);
                    changed = true;
                }
            }
            CephClusterState::Unreachable => {
                if cluster.state != Some(CephClusterState::Unreachable) {
                    cluster.state = Some(CephClusterState::Unreachable);
                    changed = true;
                }
            }
            _ => {}
        }
    }
    changed
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
        let cluster = config.clusters.entry(fsid.to_string()).or_insert_with(|| {
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
        assert_eq!(cfg.clusters[FSID].state, Some(CephClusterState::Detected));
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

    const NOW: i64 = 1_700_000_000;

    /// A registry with one detected cluster whose members live on `pve-east`.
    fn detected_cfg() -> CephClustersConfig {
        let mut cfg = CephClustersConfig::default();
        upsert_cluster(&mut cfg, FSID, &members());
        cfg
    }

    fn outcome_map(pairs: Vec<(&str, ProbeOutcome)>) -> BTreeMap<String, ProbeOutcome> {
        pairs.into_iter().map(|(r, o)| (r.to_string(), o)).collect()
    }

    #[test]
    fn reconcile_marks_gone_when_all_member_remotes_reachable() {
        let mut cfg = detected_cfg();
        let outcomes = outcome_map(vec![("pve-east", ProbeOutcome::Reachable)]);
        assert!(reconcile_unfound(
            &mut cfg,
            &BTreeMap::new(),
            &outcomes,
            NOW
        ));
        assert_eq!(cfg.clusters[FSID].state, Some(CephClusterState::Gone));
        assert_eq!(cfg.clusters[FSID].last_seen_missing, Some(NOW));
    }

    #[test]
    fn reconcile_marks_unreachable_and_leaves_last_seen() {
        let mut cfg = detected_cfg();
        let outcomes = outcome_map(vec![("pve-east", ProbeOutcome::Unreachable)]);
        assert!(reconcile_unfound(
            &mut cfg,
            &BTreeMap::new(),
            &outcomes,
            NOW
        ));
        assert_eq!(
            cfg.clusters[FSID].state,
            Some(CephClusterState::Unreachable)
        );
        assert_eq!(cfg.clusters[FSID].last_seen_missing, None);
    }

    #[test]
    fn reconcile_skips_cluster_without_coverage() {
        let mut cfg = detected_cfg();
        // Only an unrelated remote was probed: no signal for this cluster.
        let outcomes = outcome_map(vec![("pve-west", ProbeOutcome::Reachable)]);
        assert!(!reconcile_unfound(
            &mut cfg,
            &BTreeMap::new(),
            &outcomes,
            NOW
        ));
        assert_eq!(cfg.clusters[FSID].state, Some(CephClusterState::Detected));
    }

    #[test]
    fn reconcile_skips_found_cluster() {
        let mut cfg = detected_cfg();
        let mut found = BTreeMap::new();
        found.insert(FSID.to_string(), members());
        let outcomes = outcome_map(vec![("pve-east", ProbeOutcome::Reachable)]);
        assert!(!reconcile_unfound(&mut cfg, &found, &outcomes, NOW));
        assert_eq!(cfg.clusters[FSID].state, Some(CephClusterState::Detected));
    }

    #[test]
    fn reconcile_skips_tombstoned_cluster() {
        let mut cfg = detected_cfg();
        cfg.clusters.get_mut(FSID).unwrap().forgotten = Some(NOW);
        let outcomes = outcome_map(vec![("pve-east", ProbeOutcome::Reachable)]);
        assert!(!reconcile_unfound(
            &mut cfg,
            &BTreeMap::new(),
            &outcomes,
            NOW
        ));
        assert_eq!(cfg.clusters[FSID].state, Some(CephClusterState::Detected));
    }

    #[test]
    fn reconcile_gone_does_not_restamp_last_seen() {
        let mut cfg = detected_cfg();
        let outcomes = outcome_map(vec![("pve-east", ProbeOutcome::Reachable)]);
        assert!(reconcile_unfound(
            &mut cfg,
            &BTreeMap::new(),
            &outcomes,
            NOW
        ));
        // A second cycle while still gone changes nothing and keeps the original
        // last_seen_missing timestamp.
        assert!(!reconcile_unfound(
            &mut cfg,
            &BTreeMap::new(),
            &outcomes,
            NOW + 500
        ));
        assert_eq!(cfg.clusters[FSID].last_seen_missing, Some(NOW));
    }
}
