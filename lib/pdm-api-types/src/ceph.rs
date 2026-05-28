//! API types for Ceph cluster management in PDM.
//!
//! Identity in PDM's Ceph layer is fsid-derived (stable, unique, survives PVE remote rename) rather
//! than operator-named. The display name is a separate field maintained on the PDM side and
//! editable by the operator.
//!
//! PDM's own Ceph config and list types (`CephCluster`, `CephMember`, `CephClusterListEntry`)
//! serialize kebab-case, matching `remotes.cfg` and the resource types. The cross-product types
//! shared with the PVE Ceph API (`CephMon`, `CephPool`, ...) keep that family's snake_case wire
//! format.
//!
//! Section-config file layout (`/etc/proxmox-datacenter-manager/ceph-clusters.cfg`):
//!
//! ```text
//! ceph-cluster: 4b4f7f8e-1234-5678-abcd-ef0123456789
//!     display-name prod-east
//!     public-network 10.10.0.0/16
//!
//! ceph-member: 4b4f7f8e-1234-5678-abcd-ef0123456789-pve-host1
//!     cluster 4b4f7f8e-1234-5678-abcd-ef0123456789
//!     kind pve
//!     remote pve-cluster-east
//!     node host1
//! ```

use serde::{Deserialize, Serialize};

use proxmox_schema::{Schema, StringSchema, Updater, api};

use crate::{Authid, PROXMOX_SAFE_ID_FORMAT};

/// Cluster identifier on the PDM side.
///
/// Holds the cluster fsid (a UUID) verbatim. Picked as identity because it is stable across PVE
/// remote renames, unique across deployments, and what we use to deduplicate auto-detection results
/// from multiple PVE remotes that see the same Ceph cluster.
pub const CEPH_CLUSTER_ID_SCHEMA: Schema = StringSchema::new("Ceph cluster identifier (fsid).")
    .format(&PROXMOX_SAFE_ID_FORMAT)
    .min_length(8)
    .max_length(64)
    .schema();

/// Member identifier (cluster fsid + "-" + node label).
///
/// Larger maximum than `CEPH_CLUSTER_ID_SCHEMA` because it concatenates the cluster id with a host
/// or arbiter label. The 80-char cap is chosen so that `/ceph/<fsid>/<member>` (the deepest ACL
/// path the Ceph layer creates) stays inside `ACL_PATH_SCHEMA`'s 128-char limit (`1 + 4 + 1 + 64 +
/// 1 + 80 = 151` would not, but the actual fsid is 36 chars so the realistic worst case is `1 + 4 +
/// 1 + 36 + 1 + 80 = 123`).
pub const CEPH_MEMBER_ID_SCHEMA: Schema = StringSchema::new("Ceph cluster member identifier.")
    .format(&PROXMOX_SAFE_ID_FORMAT)
    .min_length(8)
    .max_length(80)
    .schema();

/// Human-readable display name for a Ceph cluster.
pub const CEPH_CLUSTER_DISPLAY_NAME_SCHEMA: Schema =
    StringSchema::new("Operator-visible display name for a Ceph cluster.")
        .min_length(1)
        .max_length(64)
        .schema();

#[api]
/// Kind of a Ceph cluster member.
///
/// Determines the transport PDM uses to reach this member: `Pve` members are reached via the
/// existing PVE remote (through PVE's Ceph REST API, or the locally-routed `proxmox-ceph-node-api`
/// daemon); `Standalone` members are reached via that daemon directly.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CephMemberKind {
    /// A member backed by a PVE remote + node.
    Pve,
    /// A member backed by a standalone `proxmox-ceph-node-api` daemon.
    Standalone,
}

serde_plain::derive_display_from_serialize!(CephMemberKind);
serde_plain::derive_fromstr_from_deserialize!(CephMemberKind);

#[api]
/// Detection state of a Ceph cluster as observed by the auto-detection sweep.
///
/// `Detected`: at least one probe succeeded this cycle and reported the fsid. `Unreachable`: at
/// least one probe failed AND no probe succeeded; the cluster might still be healthy from another
/// vantage point we cannot reach. `Gone`: every probe succeeded but no remote reports the fsid -
/// the Ceph install was removed or rebuilt with a different fsid. `Tombstoned`: operator-driven
/// "Forget cluster"; the sweep ignores re-detection until an admin runs the tombstone-reset
/// command.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CephClusterState {
    /// The cluster was probed successfully in the most recent sweep cycle.
    #[default]
    Detected,
    /// All probes failed in the most recent cycle; the cluster may still be healthy from another
    /// vantage point.
    Unreachable,
    /// Every probe succeeded but no member reports the fsid; the cluster was removed or rebuilt
    /// under a different fsid.
    Gone,
    /// Operator-driven "Forget cluster"; re-detection suppressed until the tombstone is cleared via
    /// the admin CLI.
    Tombstoned,
}

serde_plain::derive_display_from_serialize!(CephClusterState);
serde_plain::derive_fromstr_from_deserialize!(CephClusterState);

#[api(
    properties: {
        id: { schema: CEPH_CLUSTER_ID_SCHEMA },
        "display-name": { schema: CEPH_CLUSTER_DISPLAY_NAME_SCHEMA, optional: true },
        "public-network": { type: String, optional: true },
        "cluster-network": { type: String, optional: true },
        "stretch-mode": { type: Boolean, optional: true },
        "tiebreaker-mon": { type: String, optional: true },
        state: { type: CephClusterState, optional: true },
        "last-seen-missing": { type: Integer, optional: true },
        forgotten: { type: Integer, optional: true },
    },
    additional_properties: true,
)]
/// A Ceph cluster registered with PDM (auto-detected, not operator-typed).
#[derive(Clone, Debug, Deserialize, Serialize, Updater, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct CephCluster {
    /// The cluster fsid; also the section identifier in `ceph-clusters.cfg`.
    #[updater(skip)]
    pub id: String,

    /// Operator-visible display name (defaults at detection time to the Ceph cluster's own
    /// configured name, falling back to "ceph-<short-fsid>").
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub display_name: Option<String>,

    /// Public network CIDR, sniffed from the cluster's `ceph.conf` at detection.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[updater(skip)]
    pub public_network: Option<String>,

    /// Cluster network CIDR, sniffed from the cluster's `ceph.conf` at detection.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[updater(skip)]
    pub cluster_network: Option<String>,

    /// Whether stretch mode is currently enabled on the cluster.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[updater(skip)]
    pub stretch_mode: Option<bool>,

    /// Name of the tiebreaker monitor, when stretch mode is enabled.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[updater(skip)]
    pub tiebreaker_mon: Option<String>,

    /// Sweep-maintained detection state. Defaults to `Detected` on insert.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[updater(skip)]
    pub state: Option<CephClusterState>,

    /// Epoch (seconds) at which the sweep first failed to find this fsid on any reachable remote.
    /// Only set when `state == Gone`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[updater(skip)]
    pub last_seen_missing: Option<i64>,

    /// Epoch (seconds) at which the operator forgot this cluster. While set the auto-detection
    /// sweep will not re-add the cluster, even if a probe re-discovers the same fsid. Cleared by
    /// the admin CLI.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[updater(skip)]
    pub forgotten: Option<i64>,
}

#[api(
    properties: {
        id: { schema: CEPH_MEMBER_ID_SCHEMA },
        cluster: { schema: CEPH_CLUSTER_ID_SCHEMA },
        kind: { type: CephMemberKind },
        remote: { type: String, optional: true },
        node: { type: String, optional: true },
        address: { type: String, optional: true },
        fingerprint: { type: String, optional: true },
        authid: { type: Authid, optional: true },
        site: { type: String, optional: true },
    },
    additional_properties: true,
)]
/// A member of a registered Ceph cluster.
///
/// Two kinds (see `CephMemberKind`): `Pve` members reference an existing PDM PVE remote + node
/// name; `Standalone` members carry their own connection details (address, fingerprint, authid).
///
/// Only one of the two field groups is populated per row; the schema permits both as optional, the
/// deserialiser does not enforce the mutual exclusion (the registry layer at
/// `server/src/ceph/registry.rs` does).
#[derive(Clone, Debug, Deserialize, Serialize, Updater, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct CephMember {
    /// Stable identifier for the member; conventionally `<cluster-fsid>-<node-label>`.
    #[updater(skip)]
    pub id: String,

    /// fsid of the cluster this member belongs to.
    #[updater(skip)]
    pub cluster: String,

    /// What kind of member: PVE-backed or standalone-daemon-backed.
    #[updater(skip)]
    pub kind: CephMemberKind,

    // PVE-kind fields.
    /// For `kind = pve`: the PDM remote id that backs this member.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub remote: Option<String>,

    /// For `kind = pve`: the node name within the PVE remote.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub node: Option<String>,

    // Standalone-kind fields.
    /// For `kind = standalone`: the daemon's host:port address.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub address: Option<String>,

    /// For `kind = standalone`: the TLS fingerprint.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fingerprint: Option<String>,

    /// For `kind = standalone`: the authid PDM uses to talk to the daemon.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub authid: Option<Authid>,

    /// Optional site label (for stretch-aware topology views).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub site: Option<String>,
}

#[api(
    properties: {
        cluster: { schema: CEPH_CLUSTER_ID_SCHEMA },
        "display-name": { type: String },
        state: { type: CephClusterState },
        "member-count": { type: Integer },
        health: { type: String, optional: true },
        remote: { type: String, optional: true },
        node: { type: String, optional: true },
        "bytes-used": { type: Integer, optional: true },
        "bytes-total": { type: Integer, optional: true },
        "bytes-avail": { type: Integer, optional: true },
        "osds-up": { type: Integer, optional: true },
        "osds-in": { type: Integer, optional: true },
        "osds-total": { type: Integer, optional: true },
        "mons-in-quorum": { type: Integer, optional: true },
        "mons-total": { type: Integer, optional: true },
        recovering: { type: Boolean, optional: true },
        degraded: { type: Boolean, optional: true },
        nearfull: { type: Boolean, optional: true },
        "problem-count": { type: Integer, optional: true },
    },
    additional_properties: true,
)]
/// Summary entry for the cluster-list view.
///
/// Aggregates persistent config (display name) with live-derived state (member count, reachability,
/// last-known health). Returned by `GET /api2/extjs/ceph/clusters`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct CephClusterListEntry {
    /// Cluster fsid.
    pub cluster: String,
    /// Operator-visible display name.
    pub display_name: String,
    /// Current detection state.
    pub state: CephClusterState,
    /// Total members registered for the cluster.
    pub member_count: i64,
    /// Last known Ceph health string (HEALTH_OK / HEALTH_WARN / HEALTH_ERR), from the cached
    /// status; absent if nothing is cached yet.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub health: Option<String>,
    /// A representative PVE remote backing the cluster, used to deep-link into the cluster's native
    /// PVE web UI. Absent if no PVE member is known.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub remote: Option<String>,
    /// A representative member node on [`Self::remote`], used to deep-link straight to that node's
    /// Ceph panel. Absent if no PVE member is known.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub node: Option<String>,
    /// Used raw capacity in bytes, from the cached status; absent if uncached.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub bytes_used: Option<i64>,
    /// Total raw capacity in bytes, from the cached status; absent if uncached.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub bytes_total: Option<i64>,
    /// Available raw capacity in bytes, from the cached status; absent if uncached.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub bytes_avail: Option<i64>,
    /// OSDs currently up, from the cached status; absent if uncached.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub osds_up: Option<i64>,
    /// OSDs currently in, from the cached status; absent if uncached.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub osds_in: Option<i64>,
    /// Total OSDs known to the cluster, from the cached status; absent if uncached.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub osds_total: Option<i64>,
    /// Monitors currently in quorum, from the cached status; absent if uncached.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub mons_in_quorum: Option<i64>,
    /// Total monitors, from the cached status; absent if uncached.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub mons_total: Option<i64>,
    /// Whether recovery or backfill is in progress (a transient activity).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub recovering: Option<bool>,
    /// Whether objects are degraded (reduced redundancy).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub degraded: Option<bool>,
    /// Whether a near-full / full health check is active (storage pressure).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub nearfull: Option<bool>,
    /// Number of active Ceph health checks, from the cached status; lets the list show a problem
    /// count without a per-cluster fetch.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub problem_count: Option<i64>,
}

#[api]
/// A single active Ceph health check.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct CephHealthCheck {
    /// Check identifier, e.g. `OSD_DOWN`.
    pub code: String,
    /// Severity (`HEALTH_WARN` / `HEALTH_ERR`).
    pub severity: String,
    /// One-line human-readable summary.
    pub summary: String,
}

#[api]
/// PG count for one placement-group state combination.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct CephPgStateGroup {
    /// State name, e.g. `active+clean`.
    pub state_name: String,
    /// Number of placement groups in this state.
    pub count: i64,
}

#[api(
    properties: {
        kind: { type: CephMemberKind },
        remote: { type: String, optional: true },
        node: { type: String, optional: true },
        site: { type: String, optional: true },
    },
    additional_properties: true,
)]
/// A registered cluster member as shown on the dashboard: a compact projection of the registry's
/// [`CephMember`] without any connection secrets.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct CephClusterMember {
    /// Whether this member is PVE-backed or standalone-daemon-backed.
    pub kind: CephMemberKind,
    /// For `kind = pve`: the PDM remote id backing this member.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    /// The node name (PVE node, or standalone host).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// Optional site label (stretch topology).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<String>,
}

#[api(
    properties: {
        fsid: { type: String },
        health: { type: String },
        checks: {
            type: Array,
            optional: true,
            items: { type: CephHealthCheck },
        },
        "bytes-total": { type: Integer },
        "bytes-used": { type: Integer },
        "bytes-avail": { type: Integer },
        "num-pools": { type: Integer },
        "num-pgs": { type: Integer },
        "pgs-by-state": {
            type: Array,
            optional: true,
            items: { type: CephPgStateGroup },
        },
        "osds-total": { type: Integer },
        "osds-up": { type: Integer },
        "osds-in": { type: Integer },
        "mons-total": { type: Integer },
        "mons-in-quorum": { type: Integer },
        "mgr-active": { type: String, optional: true },
        "mgr-standbys": {
            type: Array,
            optional: true,
            items: { type: String, description: "Standby manager name." },
        },
        "client-read-bytes-sec": { type: Integer, optional: true },
        "client-write-bytes-sec": { type: Integer, optional: true },
        "client-read-ops-sec": { type: Integer, optional: true },
        "client-write-ops-sec": { type: Integer, optional: true },
        "recovery-bytes-sec": { type: Integer, optional: true },
        "misplaced-ratio": { type: Number, optional: true },
        "degraded-ratio": { type: Number, optional: true },
        "fullest-pool": { type: String, optional: true },
        "fullest-pool-used": { type: Number, optional: true },
        version: { type: String, optional: true },
        "version-mixed": { type: Boolean, optional: true },
        members: {
            type: Array,
            optional: true,
            items: { type: CephClusterMember },
        },
    },
    additional_properties: true,
)]
/// Typed, summarized Ceph cluster status for the dashboard.
///
/// Computed server-side from the raw `ceph status` object so the UI binds to typed fields instead
/// of digging through an untyped JSON blob. Returned by `GET /ceph/clusters/{cluster}/summary`.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct CephClusterStatus {
    /// Cluster fsid.
    pub fsid: String,
    /// Overall health (`HEALTH_OK` / `HEALTH_WARN` / `HEALTH_ERR`).
    pub health: String,
    /// Active health checks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<CephHealthCheck>,
    /// Total raw capacity in bytes.
    pub bytes_total: i64,
    /// Used capacity in bytes.
    pub bytes_used: i64,
    /// Available capacity in bytes.
    pub bytes_avail: i64,
    /// Number of pools.
    pub num_pools: i64,
    /// Total number of placement groups.
    pub num_pgs: i64,
    /// PG counts grouped by state.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pgs_by_state: Vec<CephPgStateGroup>,
    /// Total OSDs known to the cluster.
    pub osds_total: i64,
    /// OSDs currently up.
    pub osds_up: i64,
    /// OSDs currently in.
    pub osds_in: i64,
    /// Total monitors.
    pub mons_total: i64,
    /// Monitors currently in quorum.
    pub mons_in_quorum: i64,
    /// Active manager, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mgr_active: Option<String>,
    /// Standby managers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mgr_standbys: Vec<String>,
    /// Client read throughput (bytes/s), when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_read_bytes_sec: Option<i64>,
    /// Client write throughput (bytes/s), when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_write_bytes_sec: Option<i64>,
    /// Client read ops/s, when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_read_ops_sec: Option<i64>,
    /// Client write ops/s, when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_write_ops_sec: Option<i64>,
    /// Recovery throughput (bytes/s), when a recovery is active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_bytes_sec: Option<i64>,
    /// Fraction of objects misplaced, when rebalancing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub misplaced_ratio: Option<f64>,
    /// Fraction of objects degraded, when degraded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degraded_ratio: Option<f64>,
    /// Name of the most-utilized pool, to surface a near-full pool that the cluster-wide capacity
    /// figure would otherwise hide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fullest_pool: Option<String>,
    /// Used percentage (0-100) of [`Self::fullest_pool`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fullest_pool_used: Option<f64>,
    /// Representative Ceph version of the cluster (from the monitors).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Whether the monitors report more than one distinct Ceph version (a mid-upgrade /
    /// version-skew signal).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub version_mixed: bool,
    /// The cluster's registered members (the PVE remotes / nodes backing it). From the PDM
    /// registry, not the live status; surfaces cross-remote membership on the overview. Filled by
    /// the summary endpoint.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<CephClusterMember>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use proxmox_schema::ApiStringFormat;

    #[test]
    fn fsid_passes_ceph_cluster_id_schema() {
        let fsid = "4b4f7f8e-1234-5678-abcd-ef0123456789";
        let schema = match CEPH_CLUSTER_ID_SCHEMA {
            Schema::String(s) => s,
            _ => panic!("expected string schema"),
        };
        if let ApiStringFormat::Pattern(re) = schema.format.unwrap() {
            assert!(
                (re.regex_obj)().is_match(fsid),
                "fsid {fsid} does not match SAFE_ID format"
            );
        } else {
            panic!("CEPH_CLUSTER_ID_SCHEMA has unexpected format kind");
        }
        assert!(fsid.len() >= schema.min_length.unwrap_or(0));
        assert!(fsid.len() <= schema.max_length.unwrap_or(usize::MAX));
    }

    #[test]
    fn member_id_with_fsid_prefix_fits_schema() {
        let member = "4b4f7f8e-1234-5678-abcd-ef0123456789-pve-host1";
        let schema = match CEPH_MEMBER_ID_SCHEMA {
            Schema::String(s) => s,
            _ => panic!("expected string schema"),
        };
        assert!(member.len() <= schema.max_length.unwrap_or(usize::MAX));
        if let ApiStringFormat::Pattern(re) = schema.format.unwrap() {
            assert!(
                (re.regex_obj)().is_match(member),
                "member id {member} does not match SAFE_ID format"
            );
        }
    }
}
