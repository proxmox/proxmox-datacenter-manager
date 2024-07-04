//! Common API crate for PDM.

pub mod acl;
pub mod api;
pub mod auth;
pub mod env;

use anyhow::Error;
use serde_json::Value;

pub(crate) async fn reload_api_certificate() -> Result<(), Error> {
    let proxy_pid = proxmox_rest_server::read_pid(pdm_buildcfg::PDM_API_PID_FN)?;
    let sock = proxmox_rest_server::ctrl_sock_from_pid(proxy_pid);
    let _: Value =
        proxmox_rest_server::send_raw_command(sock, "{\"command\":\"reload-certificate\"}\n")
            .await?;
    Ok(())
}
