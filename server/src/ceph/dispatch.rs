//! Forwarding Ceph operations to a cluster's PVE members.
//!
//! Reaches Ceph through the existing PVE client. Cluster-wide reads can go to any member;
//! [`connect_cluster`] picks a PVE member and connects to its remote. Members are tried in the
//! order the registry provides them (sorted by id, so selection is deterministic), and the first
//! one a client can be built for wins. There is no reachability-aware selection.

use std::sync::Arc;

use anyhow::{Error, bail};

use pdm_api_types::ceph::{CephMember, CephMemberKind};

use crate::connection::PveClient;

/// A PVE member of the cluster together with a client for its remote.
pub struct CephMemberClient {
    /// The member the client connects through (its `node` field is the target for node-level Ceph
    /// endpoints).
    pub member: CephMember,
    /// PVE client for the member's remote.
    pub client: Arc<PveClient>,
}

/// Connect to a PVE member of the cluster.
///
/// Takes the cluster's members as returned by the registry's `lookup_cluster` (i.e. already
/// access-checked and sorted), so callers cannot accidentally dispatch without having run the
/// access check. The remotes config is loaded once. Errors if the cluster has no PVE member we can
/// reach.
pub fn connect_cluster(members: &[CephMember]) -> Result<CephMemberClient, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let mut last_err = None;
    for member in members {
        if member.kind != CephMemberKind::Pve {
            continue;
        }
        let Some(remote_id) = member.remote.as_deref() else {
            continue;
        };
        match crate::api::pve::get_remote(&remotes, remote_id).and_then(crate::api::pve::connect) {
            Ok(client) => {
                return Ok(CephMemberClient {
                    member: member.clone(),
                    client,
                });
            }
            Err(err) => last_err = Some(err),
        }
    }
    match last_err {
        Some(err) => Err(err),
        None => bail!("ceph cluster has no PVE member to connect through"),
    }
}
