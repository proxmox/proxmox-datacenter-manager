use proxmox_router::list_subdirs_api_method;
use proxmox_router::{Router, SubdirMap};
use proxmox_sortable_macro::sortable;

mod ad;
mod ldap;
pub mod tfa;

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("tfa", &tfa::ROUTER),
    ("ldap", &ldap::ROUTER),
    ("ad", &ad::ROUTER),
]);

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);
