//! Unprivileged PDM API library.

use anyhow::Error;

use proxmox_router::RpcEnvironment;
use proxmox_schema::api;

use pdm_config::remotes::Remote;

#[api]
/// List all the remotes this instance is managing.
pub fn list_remotes(rpcenv: &mut dyn RpcEnvironment) -> Result<Vec<Remote>, Error> {
    let (remotes, digest) = pdm_config::remotes::config()?;

    rpcenv["digest"] = digest.to_hex().into();

    Ok(remotes.into_iter().map(|(_id, value)| value).collect())
}
