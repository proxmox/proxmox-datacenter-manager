use proxmox_router::{list_subdirs_api_method, Router, SubdirMap};
use proxmox_sortable_macro::sortable;

pub mod zones;

#[sortable]
pub const SUBDIRS: SubdirMap = &sorted!([
    ("zones", &zones::ROUTER),
]);

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);
