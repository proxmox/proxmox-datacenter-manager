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

use pdm_api_types::ceph::{CephClusterListEntry, CEPH_CLUSTER_ID_SCHEMA};
use pdm_api_types::{Authid, PRIV_RESOURCE_AUDIT};

use pve_api_types::{CephFlagInfo, CephMon, CephPool};

use crate::ceph::dispatch::{self, CephMemberClient};
use crate::ceph::registry;

fn auth_id(rpcenv: &dyn RpcEnvironment) -> Result<Authid, Error> {
    rpcenv
        .get_auth_id()
        .context("no authid available")?
        .parse()
        .map_err(Error::from)
}

/// Enforce audit access on the cluster and connect through one of its PVE
/// members. Every cluster read goes through here, so the access check cannot be
/// skipped.
fn connect(auth_id: &Authid, cluster: &str) -> Result<CephMemberClient, Error> {
    let (config, _) = pdm_config::ceph::config()?;
    let (_cluster, members) =
        registry::lookup_cluster(auth_id, &config, cluster, PRIV_RESOURCE_AUDIT)?;
    dispatch::connect_cluster(&members)
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

    Ok(accessible
        .into_iter()
        .map(|(cluster, members)| {
            let display_name = cluster.display_name.clone().unwrap_or_else(|| {
                format!("ceph-{}", cluster.id.get(..8).unwrap_or(&cluster.id))
            });
            CephClusterListEntry {
                cluster: cluster.id,
                display_name,
                state: cluster.state.unwrap_or_default(),
                member_count: members.len() as i64,
                // Live reachability and health come from the status cache, which
                // is not wired yet; reported as unknown for now.
                reachable_member_count: 0,
                health: None,
            }
        })
        .collect())
}

#[api(
    input: { properties: { cluster: { schema: CEPH_CLUSTER_ID_SCHEMA } } },
    returns: { type: Object, description: "Raw `ceph status` output.", properties: {} },
    access: { permission: &Permission::Anybody },
)]
/// Cluster-wide Ceph status (the raw `ceph status` object).
pub async fn get_status(cluster: String, rpcenv: &mut dyn RpcEnvironment) -> Result<Value, Error> {
    let conn = connect(&auth_id(rpcenv)?, &cluster)?;
    conn.client
        .cluster_ceph_status()
        .await
        .with_context(|| format!("ceph cluster {cluster}: failed to read status"))
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
