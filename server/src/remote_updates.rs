use anyhow::{bail, Error};
use pdm_api_types::RemoteUpid;

use proxmox_apt_api_types::APTUpdateInfo;

use pdm_api_types::remotes::{Remote, RemoteType};

use crate::api::pve::new_remote_upid;
use crate::connection;

/// Return a list of available updates for a given remote node.
pub async fn list_available_updates(
    remote: Remote,
    node: &str,
) -> Result<Vec<APTUpdateInfo>, Error> {
    let updates = fetch_available_updates(remote, node.to_string()).await?;
    Ok(updates)
}

/// Trigger `apt update` on a remote node.
///
/// The function returns a `[RemoteUpid]` for the started update task.
pub async fn update_apt_database(remote: &Remote, node: &str) -> Result<RemoteUpid, Error> {
    match remote.ty {
        RemoteType::Pve => {
            let client = connection::make_pve_client(remote)?;

            let params = pve_api_types::AptUpdateParams {
                notify: Some(false),
                quiet: Some(false),
            };
            let upid = client.update_apt_database(node, params).await?;

            new_remote_upid(remote.id.clone(), upid).await
        }
        RemoteType::Pbs => bail!("PBS is not supported yet"),
    }
}

/// Get the changelog for a given package.
pub async fn get_changelog(remote: Remote, node: &str, package: String) -> Result<String, Error> {
    match remote.ty {
        RemoteType::Pve => {
            let client = connection::make_pve_client(&remote)?;

            client
                .get_package_changelog(node, package, None)
                .await
                .map_err(Into::into)
        }
        RemoteType::Pbs => bail!("PBS is not supported yet"),
    }
}

async fn fetch_available_updates(
    remote: Remote,
    node: String,
) -> Result<Vec<APTUpdateInfo>, Error> {
    match remote.ty {
        RemoteType::Pve => {
            let client = connection::make_pve_client(&remote)?;

            let updates = client
                .list_available_updates(&node)
                .await?
                .into_iter()
                .map(map_pve_update_info)
                .collect();

            Ok(updates)
        }
        RemoteType::Pbs => bail!("PBS is not supported yet"),
    }
}

fn map_pve_update_info(info: pve_api_types::AptUpdateInfo) -> APTUpdateInfo {
    APTUpdateInfo {
        package: info.package,
        title: info.title,
        arch: info.arch,
        description: info.description,
        version: info.version,
        old_version: info.old_version.unwrap_or_default(),
        origin: info.origin,
        priority: info.priority,
        section: info.section,
        extra_info: None,
    }
}
