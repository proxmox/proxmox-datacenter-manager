use proxmox_router::list_subdirs_api_method;
use proxmox_router::{Router, SubdirMap};
use proxmox_sortable_macro::sortable;

pub mod acme;
pub mod certificate;
pub mod notes;

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("acme", &acme::ROUTER),
    ("certificate", &certificate::ROUTER),
    ("notes", &notes::ROUTER),
]);

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);
