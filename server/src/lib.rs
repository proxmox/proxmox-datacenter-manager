//! Common API crate for PDM.

pub mod acl;
pub mod api;
pub mod auth;
pub mod context;
pub mod env;
pub mod metric_collection;
pub mod remote_cache;
pub mod remote_tasks;
pub mod resource_cache;
pub mod task_utils;

pub mod connection;
pub mod pbs_client;

#[cfg(any(remote_config = "faked", test))]
pub mod test_support;

use anyhow::Error;
use serde_json::Value;

pub(crate) async fn reload_api_certificate() -> Result<(), Error> {
    let proxy_pid = proxmox_rest_server::read_pid(pdm_buildcfg::PDM_API_PID_FN)?;
    let sock = proxmox_daemon::command_socket::path_from_pid(proxy_pid);
    let _: Value =
        proxmox_daemon::command_socket::send_raw(sock, "{\"command\":\"reload-certificate\"}\n")
            .await?;
    Ok(())
}
