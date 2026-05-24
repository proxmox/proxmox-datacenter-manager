//! `/ceph/clusters` - list registered Ceph clusters and read their state.
//!
//! Read-only (phase 1b). Cluster reads are forwarded through a PVE member of
//! the cluster (see [`crate::ceph::dispatch`]); access is enforced per cluster
//! via the registry, derived from the underlying remote's `Resource.Audit`.

use anyhow::{Context, Error};
use serde_json::Value;

use proxmox_router::{list_subdirs_api_method, Permission, Router, RpcEnvironment, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use pdm_api_types::ceph::{
    CephClusterListEntry, CephClusterMember, CephClusterStatus, CephMember, CEPH_CLUSTER_ID_SCHEMA,
};
use pdm_api_types::{Authid, PRIV_RESOURCE_AUDIT};

use pve_api_types::{CephFlagInfo, CephFs, CephMds, CephMgr, CephMon, CephPool};

use crate::ceph::dispatch::{self, CephMemberClient};
use crate::ceph::{cache, registry};

/// Default freshness window for the cluster status read when the caller does not pass `max-age`.
const DEFAULT_STATUS_MAX_AGE: u64 = 30;
/// Freshness window for the status the cluster list overlays (health, quorum). The list never
/// fetches live; it shows last-known health refreshed by the periodic detection sweep. Kept above
/// twice the sweep interval (300s) so a single missed sweep does not blank a cluster's health to
/// "unknown".
const LIST_STATUS_MAX_AGE: i64 = 660;

fn auth_id(rpcenv: &dyn RpcEnvironment) -> Result<Authid, Error> {
    rpcenv
        .get_auth_id()
        .context("no authid available")?
        .parse()
        .map_err(Error::from)
}

/// Enforce audit access on the cluster and return its members. Every cluster
/// read goes through here, so the access check cannot be skipped.
fn access_checked_members(auth_id: &Authid, cluster: &str) -> Result<Vec<CephMember>, Error> {
    let (config, _) = pdm_config::ceph::config()?;
    let (_cluster, members) =
        registry::lookup_cluster(auth_id, &config, cluster, PRIV_RESOURCE_AUDIT)?;
    Ok(members)
}

/// Access-check the cluster and connect through one of its PVE members.
fn connect(auth_id: &Authid, cluster: &str) -> Result<CephMemberClient, Error> {
    dispatch::connect_cluster(&access_checked_members(auth_id, cluster)?)
}

#[api(
    returns: {
        type: Array,
        description: "Registered Ceph clusters the caller can access.",
        items: { type: CephClusterListEntry },
    },
    access: {
        description: "Returns only clusters with at least one auditable PVE member.",
        permission: &Permission::Anybody,
    },
)]
/// List registered Ceph clusters, filtered to those the caller can audit.
pub async fn list_clusters(
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<CephClusterListEntry>, Error> {
    let auth_id = auth_id(rpcenv)?;
    let (config, _) = pdm_config::ceph::config()?;
    let accessible = registry::accessible_clusters(&auth_id, &config, PRIV_RESOURCE_AUDIT)?;

    let mut out = Vec::with_capacity(accessible.len());
    for (cluster, members) in accessible {
        // Overlay the cached status (populated by detection and status reads); never fetch live
        // here. Absent/stale cache -> unknown. The summary gives the at-a-glance triage signals
        // (health, capacity, problem count) without a per-cluster fetch.
        let summary = cache::cached_status(&cluster.id, LIST_STATUS_MAX_AGE)
            .await
            .ok()
            .flatten()
            .map(|raw| cache::summarize_status(&cluster.id, &raw));
        let health = summary.as_ref().map(|s| s.health.clone());
        let bytes_used = summary.as_ref().map(|s| s.bytes_used);
        let bytes_total = summary.as_ref().map(|s| s.bytes_total);
        let bytes_avail = summary.as_ref().map(|s| s.bytes_avail);
        let osds_up = summary.as_ref().map(|s| s.osds_up);
        let osds_in = summary.as_ref().map(|s| s.osds_in);
        let osds_total = summary.as_ref().map(|s| s.osds_total);
        let mons_in_quorum = summary.as_ref().map(|s| s.mons_in_quorum);
        let mons_total = summary.as_ref().map(|s| s.mons_total);
        // Activity signals for at-a-glance triage: recovery/backfill running, reduced redundancy,
        // or storage pressure (any *FULL* check).
        let recovering = summary
            .as_ref()
            .map(|s| s.recovery_bytes_sec.is_some() || s.misplaced_ratio.is_some_and(|r| r > 0.0));
        let degraded = summary
            .as_ref()
            .map(|s| s.degraded_ratio.is_some_and(|r| r > 0.0));
        let nearfull = summary
            .as_ref()
            .map(|s| s.checks.iter().any(|c| c.code.contains("FULL")));
        let problem_count = summary.as_ref().map(|s| s.checks.len() as i64);

        let display_name = cluster
            .display_name
            .clone()
            .unwrap_or_else(|| format!("ceph-{}", cluster.id.get(..8).unwrap_or(&cluster.id)));
        let remote = members.iter().find_map(|m| m.remote.clone());
        // A node on that representative remote, to deep-link into its Ceph panel.
        let node = remote.as_deref().and_then(|r| {
            members
                .iter()
                .find(|m| m.remote.as_deref() == Some(r))
                .and_then(|m| m.node.clone())
        });
        out.push(CephClusterListEntry {
            cluster: cluster.id,
            display_name,
            state: cluster.state.unwrap_or_default(),
            member_count: members.len() as i64,
            health,
            remote,
            node,
            bytes_used,
            bytes_total,
            bytes_avail,
            osds_up,
            osds_in,
            osds_total,
            mons_in_quorum,
            mons_total,
            recovering,
            degraded,
            nearfull,
            problem_count,
        });
    }
    Ok(out)
}

/// The raw `ceph status` for a cluster, served from the cache within `max_age` seconds or fetched
/// through a member and cached.
async fn cached_or_fetch_status(
    cluster: &str,
    members: &[CephMember],
    max_age: i64,
) -> Result<Value, Error> {
    if let Some(cached) = cache::cached_status(cluster, max_age).await? {
        return Ok(cached);
    }
    let status = dispatch::connect_cluster(members)?
        .client
        .cluster_ceph_status()
        .await
        .with_context(|| format!("ceph cluster {cluster}: failed to read status"))?;
    cache::store_status(cluster, &status).await?;
    Ok(status)
}

#[api(
    input: {
        properties: {
            cluster: { schema: CEPH_CLUSTER_ID_SCHEMA },
            "max-age": {
                type: Integer,
                optional: true,
                minimum: 0,
                description: "Serve a cached status if it is younger than this many seconds.",
            },
        },
    },
    returns: { type: Object, description: "Raw `ceph status` output.", properties: {} },
    access: { permission: &Permission::Anybody },
)]
/// Cluster-wide Ceph status (the raw `ceph status` object), served from the cache within `max-age`
/// seconds or fetched fresh and cached.
pub async fn get_status(
    cluster: String,
    max_age: Option<u64>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Value, Error> {
    let members = access_checked_members(&auth_id(rpcenv)?, &cluster)?;
    let max_age = max_age.unwrap_or(DEFAULT_STATUS_MAX_AGE) as i64;
    cached_or_fetch_status(&cluster, &members, max_age).await
}

#[api(
    input: {
        properties: {
            cluster: { schema: CEPH_CLUSTER_ID_SCHEMA },
            "max-age": {
                type: Integer,
                optional: true,
                minimum: 0,
                description: "Serve a cached status if it is younger than this many seconds.",
            },
        },
    },
    returns: { type: CephClusterStatus },
    access: { permission: &Permission::Anybody },
)]
/// Typed, summarized Ceph cluster status (health, capacity, OSD/MON/MGR/PG counts) for the
/// dashboard. Most fields come from the cached `ceph status` (so the UI binds typed fields instead
/// of parsing a raw blob); the fullest-pool and version fields are enriched from a live,
/// best-effort member read (see [`enrich_summary`]).
pub async fn get_summary(
    cluster: String,
    max_age: Option<u64>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<CephClusterStatus, Error> {
    let members = access_checked_members(&auth_id(rpcenv)?, &cluster)?;
    let max_age = max_age.unwrap_or(DEFAULT_STATUS_MAX_AGE) as i64;
    let raw = cached_or_fetch_status(&cluster, &members, max_age).await?;
    let mut summary = cache::summarize_status(&cluster, &raw);
    enrich_summary(&cluster, &members, &mut summary).await;
    // Surface the registered membership on the overview; the members are already loaded above for
    // the access check, so this is free.
    summary.members = members
        .iter()
        .map(|m| CephClusterMember {
            kind: m.kind,
            remote: m.remote.clone(),
            node: m.node.clone(),
            site: m.site.clone(),
        })
        .collect();
    Ok(summary)
}

/// Add the fullest-pool and Ceph-version signals to a dashboard summary.
///
/// Both need a live member call (the cached `ceph status` carries neither), so this is best-effort:
/// a failure must not blank the dashboard, it just leaves the extra fields unset. The cluster list
/// never calls this, so the cheap overview stays fetch-free.
async fn enrich_summary(cluster: &str, members: &[CephMember], summary: &mut CephClusterStatus) {
    let conn = match dispatch::connect_cluster(members) {
        Ok(conn) => conn,
        Err(_) => return,
    };
    let Some(node) = conn.member.node.as_deref() else {
        return;
    };

    match conn.client.list_ceph_pools(node).await {
        Ok(pools) => {
            if let Some((name, used)) = pools
                .iter()
                .filter_map(|p| {
                    p.percent_used
                        .filter(|u| u.is_finite())
                        .map(|u| (p.pool_name.clone(), u * 100.0))
                })
                .max_by(|a, b| a.1.total_cmp(&b.1))
            {
                summary.fullest_pool = Some(name);
                summary.fullest_pool_used = Some(used);
            }
        }
        Err(e) => log::warn!("ceph cluster {cluster}: fullest-pool lookup failed: {e}"),
    }

    match conn.client.list_ceph_mon(node).await {
        Ok(mons) => {
            let versions: std::collections::BTreeSet<String> = mons
                .iter()
                .filter_map(|m| m.ceph_version_short.clone())
                .collect();
            summary.version_mixed = versions.len() > 1;
            summary.version = versions.into_iter().next_back();
        }
        Err(e) => log::warn!("ceph cluster {cluster}: version lookup failed: {e}"),
    }
}

#[api(
    input: { properties: { cluster: { schema: CEPH_CLUSTER_ID_SCHEMA } } },
    returns: {
        type: Array,
        description: "Ceph OSD flags and their state.",
        items: { type: CephFlagInfo },
    },
    access: { permission: &Permission::Anybody },
)]
/// Cluster-wide Ceph flags.
pub async fn list_flags(
    cluster: String,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<CephFlagInfo>, Error> {
    let conn = connect(&auth_id(rpcenv)?, &cluster)?;
    conn.client
        .cluster_ceph_flags()
        .await
        .with_context(|| format!("ceph cluster {cluster}: failed to read flags"))
}

#[api(
    input: { properties: { cluster: { schema: CEPH_CLUSTER_ID_SCHEMA } } },
    returns: {
        type: Array,
        description: "Ceph monitors of the cluster.",
        items: { type: CephMon },
    },
    access: { permission: &Permission::Anybody },
)]
/// List the cluster's Ceph monitors.
pub async fn list_mon(
    cluster: String,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<CephMon>, Error> {
    let conn = connect(&auth_id(rpcenv)?, &cluster)?;
    let node = conn
        .member
        .node
        .as_deref()
        .context("pve ceph member has no node")?;
    conn.client
        .list_ceph_mon(node)
        .await
        .with_context(|| format!("ceph cluster {cluster}: failed to list monitors"))
}

#[api(
    input: { properties: { cluster: { schema: CEPH_CLUSTER_ID_SCHEMA } } },
    returns: {
        type: Array,
        description: "Ceph managers of the cluster.",
        items: { type: CephMgr },
    },
    access: { permission: &Permission::Anybody },
)]
/// List the cluster's Ceph managers.
pub async fn list_mgr(
    cluster: String,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<CephMgr>, Error> {
    let conn = connect(&auth_id(rpcenv)?, &cluster)?;
    let node = conn
        .member
        .node
        .as_deref()
        .context("pve ceph member has no node")?;
    conn.client
        .list_ceph_mgr(node)
        .await
        .with_context(|| format!("ceph cluster {cluster}: failed to list managers"))
}

#[api(
    input: { properties: { cluster: { schema: CEPH_CLUSTER_ID_SCHEMA } } },
    returns: {
        type: Array,
        description: "Ceph metadata servers of the cluster.",
        items: { type: CephMds },
    },
    access: { permission: &Permission::Anybody },
)]
/// List the cluster's Ceph metadata servers (MDS).
pub async fn list_mds(
    cluster: String,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<CephMds>, Error> {
    let conn = connect(&auth_id(rpcenv)?, &cluster)?;
    let node = conn
        .member
        .node
        .as_deref()
        .context("pve ceph member has no node")?;
    conn.client
        .list_ceph_mds(node)
        .await
        .with_context(|| format!("ceph cluster {cluster}: failed to list metadata servers"))
}

#[api(
    input: { properties: { cluster: { schema: CEPH_CLUSTER_ID_SCHEMA } } },
    returns: {
        type: Array,
        description: "CephFS file systems of the cluster.",
        items: { type: CephFs },
    },
    access: { permission: &Permission::Anybody },
)]
/// List the cluster's CephFS file systems.
pub async fn list_fs(
    cluster: String,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<CephFs>, Error> {
    let conn = connect(&auth_id(rpcenv)?, &cluster)?;
    let node = conn
        .member
        .node
        .as_deref()
        .context("pve ceph member has no node")?;
    conn.client
        .list_ceph_fs(node)
        .await
        .with_context(|| format!("ceph cluster {cluster}: failed to list file systems"))
}

#[api(
    input: { properties: { cluster: { schema: CEPH_CLUSTER_ID_SCHEMA } } },
    returns: {
        type: Array,
        description: "Ceph pools of the cluster.",
        items: { type: CephPool },
    },
    access: { permission: &Permission::Anybody },
)]
/// List the cluster's Ceph pools.
pub async fn list_pools(
    cluster: String,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<CephPool>, Error> {
    let conn = connect(&auth_id(rpcenv)?, &cluster)?;
    let node = conn
        .member
        .node
        .as_deref()
        .context("pve ceph member has no node")?;
    conn.client
        .list_ceph_pools(node)
        .await
        .with_context(|| format!("ceph cluster {cluster}: failed to list pools"))
}

#[api(
    input: { properties: { cluster: { schema: CEPH_CLUSTER_ID_SCHEMA } } },
    returns: { type: Object, description: "Ceph CRUSH/OSD tree.", properties: {} },
    access: { permission: &Permission::Anybody },
)]
/// The cluster's OSD (CRUSH) tree.
pub async fn get_osd_tree(
    cluster: String,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Value, Error> {
    let conn = connect(&auth_id(rpcenv)?, &cluster)?;
    let node = conn
        .member
        .node
        .as_deref()
        .context("pve ceph member has no node")?;
    conn.client
        .get_ceph_osd_tree(node)
        .await
        .with_context(|| format!("ceph cluster {cluster}: failed to read OSD tree"))
}

#[sortable]
const CLUSTER_SUBDIRS: SubdirMap = &sorted!([
    ("flags", &Router::new().get(&API_METHOD_LIST_FLAGS)),
    ("fs", &Router::new().get(&API_METHOD_LIST_FS)),
    ("mds", &Router::new().get(&API_METHOD_LIST_MDS)),
    ("mgr", &Router::new().get(&API_METHOD_LIST_MGR)),
    ("mon", &Router::new().get(&API_METHOD_LIST_MON)),
    ("osd-tree", &Router::new().get(&API_METHOD_GET_OSD_TREE)),
    ("pools", &Router::new().get(&API_METHOD_LIST_POOLS)),
    ("status", &Router::new().get(&API_METHOD_GET_STATUS)),
    ("summary", &Router::new().get(&API_METHOD_GET_SUMMARY)),
]);

const CLUSTER_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(CLUSTER_SUBDIRS))
    .subdirs(CLUSTER_SUBDIRS);

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_CLUSTERS)
    .match_all("cluster", &CLUSTER_ROUTER);
