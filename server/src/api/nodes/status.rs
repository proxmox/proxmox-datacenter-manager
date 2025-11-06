use pdm_api_types::{PRIV_SYS_AUDIT, PRIV_SYS_POWER_MANAGEMENT};
use proxmox_router::{ApiMethod, Permission, Router};

const API_METHOD_GET_STATUS_WITH_ACCESS: ApiMethod = proxmox_node_status::API_METHOD_GET_STATUS
    .access(
        None,
        &Permission::Privilege(&["system", "status"], PRIV_SYS_AUDIT, false),
    );

const API_METHOD_REBOOT_OR_SHUTDOWN_WITH_ACCESS: ApiMethod =
    proxmox_node_status::API_METHOD_REBOOT_OR_SHUTDOWN.access(
        None,
        &Permission::Privilege(&["system", "status"], PRIV_SYS_POWER_MANAGEMENT, false),
    );

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_GET_STATUS_WITH_ACCESS)
    .post(&API_METHOD_REBOOT_OR_SHUTDOWN_WITH_ACCESS);
