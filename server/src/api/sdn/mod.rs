use proxmox_router::{list_subdirs_api_method, Router, SubdirMap};
use proxmox_sortable_macro::sortable;

pub mod controllers;
pub mod vnets;
pub mod zones;

#[sortable]
pub const SUBDIRS: SubdirMap = &sorted!([
    ("controllers", &controllers::ROUTER),
    ("vnets", &vnets::ROUTER),
    ("zones", &zones::ROUTER),
]);

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);
