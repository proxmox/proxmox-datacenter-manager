use anyhow::Error;

use proxmox_router::{list_subdirs_api_method, Router, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use pdm_api_types::{remotes::REMOTE_ID_SCHEMA, NODE_SCHEMA};

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([("rrddata", &super::rrddata::NODE_RRD_ROUTER),]);
