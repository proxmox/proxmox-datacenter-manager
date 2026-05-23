//! Datacenter-wide Ceph management UI.
//!
//! Top-level, fsid-keyed view (Ceph clusters can span several PVE remotes, so
//! this lives parallel to Remotes/SDN rather than under a single remote).
//! Phase 1b is read-only.

pub mod cluster;
pub mod clusters;
pub mod dashboard;
pub mod monitors;
pub mod osds;
pub mod renderer;
pub mod view;

pub use view::CephView;
