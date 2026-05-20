//! Read-only Ceph API surface (phase 1b).
//!
//! Exposes the registry of detected Ceph clusters and forwards status reads to
//! a PVE member of each cluster. See [`crate::ceph`] for the access model and
//! dispatch.

use proxmox_router::{list_subdirs_api_method, Router, SubdirMap};
use proxmox_sortable_macro::sortable;

pub mod clusters;

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([("clusters", &clusters::ROUTER)]);

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);
