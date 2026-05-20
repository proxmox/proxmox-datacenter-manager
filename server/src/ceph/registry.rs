//! Access-checked view over the Ceph cluster registry (`ceph-clusters.cfg`).
//!
//! Access is derived from the underlying PVE remote (see [`super`] module
//! docs): a member is accessible when the caller holds the required privilege
//! on `/resource/<remote>` via [`CachedUserInfo::any_privs_below`] - the same
//! predicate the resource list uses to decide remote visibility, so "if you can
//! see the remote you can see its Ceph" holds - and a cluster is accessible when
//! at least one of its members is. The privilege is a parameter so the same
//! checks back read (`Resource.Audit`) and, later, write (`Resource.Modify`)
//! flows; when a dedicated `/ceph` ACL path family is introduced, only the
//! bodies of [`check_cluster_access`] / [`check_member_access`] change.
//!
//! Known limitation, intentional for the read-only phase: a Ceph cluster can
//! span several PVE remotes, and cluster-wide reads (`ceph status`, mon/osd
//! lists) describe the whole cluster. A caller who can audit one member's
//! remote therefore sees cluster-wide state that includes nodes on remotes they
//! cannot audit. The plan's per-member intersection filtering is a later
//! refinement that arrives with the dedicated `/ceph` ACL paths.

use anyhow::Error;

use proxmox_access_control::CachedUserInfo;
use proxmox_router::http_bail;

use pdm_api_types::ceph::{CephCluster, CephMember, CephMemberKind};
use pdm_api_types::Authid;
use pdm_config::ceph::CephClustersConfig;

/// Whether the caller holds `privs` on the remote backing `member`.
///
/// Standalone members have no remote to derive from and are inaccessible until
/// the dedicated `/ceph` ACL path family lands.
fn member_accessible(
    user_info: &CachedUserInfo,
    auth_id: &Authid,
    member: &CephMember,
    privs: u64,
) -> Result<bool, Error> {
    match (member.kind, member.remote.as_deref()) {
        (CephMemberKind::Pve, Some(remote)) => {
            user_info.any_privs_below(auth_id, &["resource", remote], privs)
        }
        _ => Ok(false),
    }
}

/// Whether the caller holds `privs` on at least one of the cluster's members.
pub fn cluster_accessible(
    user_info: &CachedUserInfo,
    auth_id: &Authid,
    config: &CephClustersConfig,
    cluster_id: &str,
    privs: u64,
) -> Result<bool, Error> {
    for member in config.members_of(cluster_id) {
        if member_accessible(user_info, auth_id, member, privs)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Enforce that the caller holds `privs` on the cluster (via any member's
/// remote). `404` when the cluster is unknown, `403` when it is not accessible.
pub fn check_cluster_access(
    auth_id: &Authid,
    config: &CephClustersConfig,
    cluster_id: &str,
    privs: u64,
) -> Result<(), Error> {
    if !config.clusters.contains_key(cluster_id) {
        http_bail!(NOT_FOUND, "no such ceph cluster {cluster_id:?}");
    }
    let user_info = CachedUserInfo::new()?;
    if !cluster_accessible(&user_info, auth_id, config, cluster_id, privs)? {
        http_bail!(FORBIDDEN, "no access to ceph cluster {cluster_id:?}");
    }
    Ok(())
}

/// Enforce that the caller holds `privs` on the given member's remote.
pub fn check_member_access(auth_id: &Authid, member: &CephMember, privs: u64) -> Result<(), Error> {
    let user_info = CachedUserInfo::new()?;
    if !member_accessible(&user_info, auth_id, member, privs)? {
        http_bail!(FORBIDDEN, "no access to ceph member {:?}", member.id);
    }
    Ok(())
}

/// Members of a cluster, cloned and sorted by id so dispatch and listing are
/// deterministic (the backing store is a `HashMap`).
fn sorted_members(config: &CephClustersConfig, cluster_id: &str) -> Vec<CephMember> {
    let mut members: Vec<CephMember> = config.members_of(cluster_id).cloned().collect();
    members.sort_by(|a, b| a.id.cmp(&b.id));
    members
}

/// Clusters the caller can access with `privs`, each paired with its members.
/// Clusters and members are sorted by id for deterministic output.
pub fn accessible_clusters(
    auth_id: &Authid,
    config: &CephClustersConfig,
    privs: u64,
) -> Result<Vec<(CephCluster, Vec<CephMember>)>, Error> {
    let user_info = CachedUserInfo::new()?;
    let mut out = Vec::new();
    for (id, cluster) in &config.clusters {
        if cluster_accessible(&user_info, auth_id, config, id, privs)? {
            out.push((cluster.clone(), sorted_members(config, id)));
        }
    }
    out.sort_by(|a, b| a.0.id.cmp(&b.0.id));
    Ok(out)
}

/// Look up a single accessible cluster and its members (sorted), enforcing
/// `privs`. `404`/`403` as in [`check_cluster_access`].
pub fn lookup_cluster(
    auth_id: &Authid,
    config: &CephClustersConfig,
    cluster_id: &str,
    privs: u64,
) -> Result<(CephCluster, Vec<CephMember>), Error> {
    check_cluster_access(auth_id, config, cluster_id, privs)?;
    // check_cluster_access has confirmed the cluster exists.
    let cluster = config.clusters[cluster_id].clone();
    Ok((cluster, sorted_members(config, cluster_id)))
}
