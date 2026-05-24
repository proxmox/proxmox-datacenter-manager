//! Ceph cluster API surface.
//!
//! Exposes the registry of detected Ceph clusters and forwards per-cluster reads and operations to
//! a PVE member of each cluster. See [`crate::ceph`] for the access model and dispatch.

use proxmox_router::{list_subdirs_api_method, Router, SubdirMap};
use proxmox_sortable_macro::sortable;

pub mod clusters;

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([("clusters", &clusters::ROUTER)]);

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);
