//! API for getting a remote update update summary.

use anyhow::Error;

use pdm_api_types::remote_updates::UpdateSummary;
use pdm_api_types::remotes::Remote;
use pdm_api_types::{PRIV_RESOURCE_MODIFY, UPID};
use proxmox_access_control::CachedUserInfo;
use proxmox_rest_server::WorkerTask;
use proxmox_router::{
    http_bail, list_subdirs_api_method, Permission, Router, RpcEnvironment, SubdirMap,
};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use crate::remote_updates;

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("summary", &Router::new().get(&API_METHOD_UPDATE_SUMMARY)),
    (
        "refresh",
        &Router::new().post(&API_METHOD_REFRESH_REMOTE_UPDATE_SUMMARIES)
    ),
]);

#[api(
    access: {
        permission: &Permission::Anybody,
        description: "Resource.Modify privileges are needed on /resource/{remote}",
    },
)]
/// Return available update summary for managed remote nodes.
pub fn update_summary(rpcenv: &mut dyn RpcEnvironment) -> Result<UpdateSummary, Error> {
    let auth_id = rpcenv.get_auth_id().unwrap().parse()?;
    let user_info = CachedUserInfo::new()?;

    if !user_info.any_privs_below(&auth_id, &["resource"], PRIV_RESOURCE_MODIFY)? {
        http_bail!(FORBIDDEN, "user has no access to resources");
    }

    let mut update_summary = remote_updates::get_available_updates_summary()?;

    update_summary.remotes.retain(|remote_name, _| {
        user_info
            .check_privs(
                &auth_id,
                &["resource", remote_name],
                PRIV_RESOURCE_MODIFY,
                false,
            )
            .is_ok()
    });

    Ok(update_summary)
}

#[api(
    access: {
        permission: &Permission::Anybody,
        description: "Resource.Modify privileges are needed on /resource/{remote}",
    },
)]
/// Refresh the update summary of all remotes.
pub fn refresh_remote_update_summaries(rpcenv: &mut dyn RpcEnvironment) -> Result<UPID, Error> {
    let (config, _digest) = pdm_config::remotes::config()?;

    let auth_id = rpcenv.get_auth_id().unwrap().parse()?;
    let user_info = CachedUserInfo::new()?;

    if !user_info.any_privs_below(&auth_id, &["resource"], PRIV_RESOURCE_MODIFY)? {
        http_bail!(FORBIDDEN, "user has no access to resources");
    }

    let remotes: Vec<Remote> = config
        .into_iter()
        .filter_map(|(remote_name, remote)| {
            user_info
                .check_privs(
                    &auth_id,
                    &["resource", &remote_name],
                    PRIV_RESOURCE_MODIFY,
                    false,
                )
                .is_ok()
                .then_some(remote)
        })
        .collect();

    let upid_str = WorkerTask::spawn(
        "refresh-remote-updates",
        None,
        auth_id.to_string(),
        true,
        |_worker| async {
            // TODO: Add more verbose logging per remote/node, so we can actually see something
            // interesting in the task log.
            remote_updates::refresh_update_summary_cache(remotes).await?;
            Ok(())
        },
    )?;

    upid_str.parse()
}
