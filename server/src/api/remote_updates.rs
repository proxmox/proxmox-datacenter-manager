//! API for getting a remote update update summary.

use anyhow::Error;

use pdm_api_types::remotes::Remote;
use pdm_api_types::{
    remote_updates::UpdateSummary, remotes::REMOTE_ID_SCHEMA, RemoteUpid, NODE_SCHEMA,
    PRIV_RESOURCE_MODIFY, UPID,
};
use proxmox_access_control::CachedUserInfo;
use proxmox_apt_api_types::{APTGetChangelogOptions, APTUpdateInfo};
use proxmox_rest_server::WorkerTask;
use proxmox_router::{
    http_bail, list_subdirs_api_method, Permission, Router, RpcEnvironment, SubdirMap,
};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use crate::remote_updates;

use super::remotes::get_remote;

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
    returns: { type: UpdateSummary }
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
    returns: { type: UPID }
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

#[api(
    input: {
        properties: {
            remote: {
                schema: REMOTE_ID_SCHEMA,
            },
            node: {
                schema: NODE_SCHEMA,
            },
        },
    },
    returns: {
        description: "A list of packages with available updates.",
        type: Array,
        items: {
            type: APTUpdateInfo
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "node", "{node}", "system"], PRIV_RESOURCE_MODIFY, false),
    },
)]
/// List available APT updates for a remote PVE node.
async fn apt_update_available(remote: String, node: String) -> Result<Vec<APTUpdateInfo>, Error> {
    let (config, _digest) = pdm_config::remotes::config()?;
    let remote = get_remote(&config, &remote)?;

    let updates = remote_updates::list_available_updates(remote.clone(), &node).await?;

    Ok(updates)
}

#[api(
    input: {
        properties: {
            remote: {
                schema: REMOTE_ID_SCHEMA,
            },
            node: {
                schema: NODE_SCHEMA,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "node", "{node}", "system"], PRIV_RESOURCE_MODIFY, false),
    },
    returns: { type: RemoteUpid }
)]
/// Update the APT database of a remote PVE node.
pub async fn apt_update_database(remote: String, node: String) -> Result<RemoteUpid, Error> {
    let (config, _digest) = pdm_config::remotes::config()?;
    let remote = get_remote(&config, &remote)?;

    let upid = remote_updates::update_apt_database(remote, &node).await?;

    Ok(upid)
}

#[api(
    input: {
        properties: {
            remote: {
                schema: REMOTE_ID_SCHEMA,
            },
            node: {
                schema: NODE_SCHEMA,
            },
            options: {
                type: APTGetChangelogOptions,
                flatten: true,
            },
        },
    },
    returns: {
        description: "The Package changelog.",
        type: String,
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "node", "{node}", "system"], PRIV_RESOURCE_MODIFY, false),
    },
)]
/// Retrieve the changelog of the specified package for a remote PVE node.
async fn apt_get_changelog(
    remote: String,
    node: String,
    options: APTGetChangelogOptions,
) -> Result<String, Error> {
    let (config, _digest) = pdm_config::remotes::config()?;
    let remote = get_remote(&config, &remote)?;

    remote_updates::get_changelog(remote, &node, options.name).await
}

const APT_SUBDIRS: SubdirMap = &[
    (
        "changelog",
        &Router::new().get(&API_METHOD_APT_GET_CHANGELOG),
    ),
    (
        "update",
        &Router::new()
            .get(&API_METHOD_APT_UPDATE_AVAILABLE)
            .post(&API_METHOD_APT_UPDATE_DATABASE),
    ),
];

pub const APT_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(APT_SUBDIRS))
    .subdirs(APT_SUBDIRS);
