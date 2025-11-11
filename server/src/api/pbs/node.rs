use proxmox_router::{list_subdirs_api_method, Router, SubdirMap};
use proxmox_sortable_macro::sortable;

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("apt", &crate::api::remote_updates::APT_ROUTER),
    (
        "termproxy",
        &Router::new().post(&crate::api::remote_shell::API_METHOD_SHELL_TICKET)
    ),
    (
        "vncwebsocket",
        &Router::new().upgrade(&crate::api::remote_shell::API_METHOD_SHELL_WEBSOCKET)
    ),
]);
