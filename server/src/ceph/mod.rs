//! Central Ceph management glue.
//!
//! Phase 1b is read-only and PVE-backed: every cluster member maps to a PVE
//! remote plus node, so PDM reaches Ceph through the existing PVE client and
//! derives access from the underlying remote's `Resource.*` privileges. A
//! dedicated `/ceph` ACL path family (with the generic `Resource.Audit`,
//! `Resource.Modify` and `Resource.Manage` privileges applied to it) is
//! intentionally deferred until there are standalone members with no remote to
//! derive from, or a need for finer-grained gating than the remote already
//! provides.
//!
//! When that path family is introduced, the convention is `/ceph/<cluster>` and
//! `/ceph/<cluster>/<member>`, where `<cluster>` is the cluster fsid and
//! `<member>` is the full member id (`<fsid>-<node-label>`, the section-config
//! key) used verbatim - no translation. That repeats the fsid in the path but
//! keeps the path component identical to the registry key, and stays within
//! `ACL_PATH_SCHEMA`'s length limit.

pub mod dispatch;
pub mod registry;
pub mod sweep;
