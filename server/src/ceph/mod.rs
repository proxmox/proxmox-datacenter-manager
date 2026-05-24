//! Central Ceph management glue.
//!
//! PVE-backed: every cluster member maps to a PVE remote plus node, so PDM reaches Ceph through the
//! existing PVE client and derives access from the underlying remote's `Resource.*` privileges. A
//! dedicated `/ceph` ACL path family (with the generic `Resource.Audit`, `Resource.Modify` and
//! `Resource.Manage` privileges) is omitted while every member derives from a remote; standalone
//! members, which have no remote to derive from, would require it.
//!
//! The convention for that path family is `/ceph/<cluster>` and `/ceph/<cluster>/<member>`, where
//! `<cluster>` is the cluster fsid and `<member>` is the full member id (`<fsid>-<node-label>`, the
//! section-config key) used verbatim, with no translation. That repeats the fsid in the path but
//! keeps the path component identical to the registry key, and stays within `ACL_PATH_SCHEMA`'s
//! length limit.

pub mod cache;
pub mod dispatch;
pub mod registry;
pub mod sweep;
