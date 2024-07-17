//! Common `/api2/*/access/ticket` router.

use proxmox_router::{list_subdirs_api_method, Router, SubdirMap};
use proxmox_sortable_macro::sortable;

mod acl;
mod domains;
mod tfa;
mod users;

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("acl", &acl::ROUTER),
    ("domains", &domains::ROUTER),
    ("tfa", &tfa::ROUTER),
    (
        "ticket",
        &Router::new().post(&proxmox_auth_api::api::API_METHOD_CREATE_TICKET),
    ),
    ("users", &users::ROUTER),
]);

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);
