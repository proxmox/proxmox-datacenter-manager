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

use pdm_api_types::ceph::{CephClusterListEntry, CephMember, CEPH_CLUSTER_ID_SCHEMA};
use pdm_api_types::{Authid, PRIV_RESOURCE_AUDIT};

use pve_api_types::{CephFlagInfo, CephMon, CephPool};

use crate::ceph::dispatch::{self, CephMemberClient};
use crate::ceph::{cache, registry};

/// Default freshness window for the cluster status read when the caller does
/// not pass `max-age`.
const DEFAULT_STATUS_MAX_AGE: u64 = 30;
/// Freshness window for the status the cluster list overlays (health, quorum).
/// Generous, since the list is a cheap overview and never fetches live.
const LIST_STATUS_MAX_AGE: i64 = 60;

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
        // Overlay the cached status (populated by detection and status reads);
        // never fetch live here. Absent/stale cache -> health unknown.
        let status = cache::cached_status(&cluster.id, LIST_STATUS_MAX_AGE)
            .await
            .ok()
            .flatten();
        let health = status.as_ref().and_then(cache::health_from_status);
        // Monitors currently in quorum, as a cluster-liveness proxy. True
        // per-member reachability arrives with the sweep's per-member probing.
        let reachable_member_count = status.as_ref().map_or(0, cache::quorum_count_from_status);

        let display_name = cluster
            .display_name
            .clone()
            .unwrap_or_else(|| format!("ceph-{}", cluster.id.get(..8).unwrap_or(&cluster.id)));
        out.push(CephClusterListEntry {
            cluster: cluster.id,
            display_name,
            state: cluster.state.unwrap_or_default(),
            member_count: members.len() as i64,
            reachable_member_count,
            health,
        });
    }
    Ok(out)
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
/// Cluster-wide Ceph status (the raw `ceph status` object), served from the
/// cache within `max-age` seconds or fetched fresh and cached.
pub async fn get_status(
    cluster: String,
    max_age: Option<u64>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Value, Error> {
    let members = access_checked_members(&auth_id(rpcenv)?, &cluster)?;

    let max_age = max_age.unwrap_or(DEFAULT_STATUS_MAX_AGE) as i64;
    if let Some(cached) = cache::cached_status(&cluster, max_age).await? {
        return Ok(cached);
    }

    let status = dispatch::connect_cluster(&members)?
        .client
        .cluster_ceph_status()
        .await
        .with_context(|| format!("ceph cluster {cluster}: failed to read status"))?;
    cache::store_status(&cluster, &status).await?;
    Ok(status)
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
    ("mon", &Router::new().get(&API_METHOD_LIST_MON)),
    ("osd-tree", &Router::new().get(&API_METHOD_GET_OSD_TREE)),
    ("pools", &Router::new().get(&API_METHOD_LIST_POOLS)),
    ("status", &Router::new().get(&API_METHOD_GET_STATUS)),
]);

const CLUSTER_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(CLUSTER_SUBDIRS))
    .subdirs(CLUSTER_SUBDIRS);

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_CLUSTERS)
    .match_all("cluster", &CLUSTER_ROUTER);
