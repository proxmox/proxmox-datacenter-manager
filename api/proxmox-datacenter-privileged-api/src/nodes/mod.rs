use proxmox_router::{list_subdirs_api_method, Router, SubdirMap};
use proxmox_sortable_macro::sortable;

pub const ROUTER: Router = Router::new()
    .get(&pdm_api_common::api::node::API_METHOD_LIST_NODES)
    .match_all("node", &ITEM_ROUTER);

pub const ITEM_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
pub const SUBDIRS: SubdirMap = &sorted!([("dns", &pdm_api_common::api::node::dns::ROUTER)]);
