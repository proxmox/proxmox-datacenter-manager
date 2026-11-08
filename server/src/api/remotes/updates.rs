//! API for getting a remote update update summary.

use anyhow::{Context, Error};

use pdm_api_types::PRIV_RESOURCE_AUDIT;
use pdm_api_types::remotes::{Remote, RemoteType};
use pdm_api_types::{
    APTRepositoriesResult, NODE_SCHEMA, PRIV_RESOURCE_MODIFY, RemoteUpid, UPID,
    remote_updates::UpdateSummary, remotes::REMOTE_ID_SCHEMA,
};
use proxmox_access_control::CachedUserInfo;
use proxmox_apt_api_types::{APTGetChangelogOptions, APTUpdateInfo};
use proxmox_apt_history::{HistoryEntry, HistoryListOptions, TermLogFormat, TransactionChangelog};
use proxmox_deb_changelog::ChangelogEntry;
use proxmox_rest_server::WorkerTask;
use proxmox_router::{
    Permission, Router, RpcEnvironment, SubdirMap, http_bail, list_subdirs_api_method,
};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use crate::api::remotes::RemoteIterator;
use crate::{connection, remote_updates};

use super::get_remote;

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
pub async fn update_summary(rpcenv: &mut dyn RpcEnvironment) -> Result<UpdateSummary, Error> {
    let auth_id = rpcenv
        .get_auth_id()
        .context("no authid available")?
        .parse()?;
    let user_info = CachedUserInfo::new()?;

    if !user_info.any_privs_below(&auth_id, &["resource"], PRIV_RESOURCE_MODIFY)? {
        http_bail!(FORBIDDEN, "user has no access to resources");
    }

    let mut update_summary = remote_updates::get_available_updates_summary().await?;

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
    let auth_id = rpcenv
        .get_auth_id()
        .context("no authid available")?
        .parse()?;
    let user_info = CachedUserInfo::new()?;

    if !user_info.any_privs_below(&auth_id, &["resource"], PRIV_RESOURCE_MODIFY)? {
        http_bail!(FORBIDDEN, "user has no access to resources");
    }

    let remotes: Vec<Remote> = RemoteIterator::new(pdm_config::remotes::instance())?
        .all_privs(&user_info, &auth_id, PRIV_RESOURCE_MODIFY)
        .into_remotes()
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

    let updates = remote_updates::list_available_updates(remote.clone(), node).await?;

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
        description: "Changelog entries for pending updates, grouped by source package.",
        type: Array,
        items: {
            type: TransactionChangelog,
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "node", "{node}", "system"], PRIV_RESOURCE_MODIFY, false),
    },
)]
/// Get the changelog entries for the pending updates of a remote node.
async fn apt_update_changelogs(
    remote: String,
    node: String,
) -> Result<Vec<TransactionChangelog>, Error> {
    let (config, _digest) = pdm_config::remotes::config()?;
    let remote = get_remote(&config, &remote)?;

    remote_updates::get_update_changelogs(remote, &node).await
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
        permission: &Permission::Privilege(&["resource", "{remote}", "node", "{node}", "system"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get configured APT repositories.
async fn get_apt_repositories(
    remote: String,
    node: String,
) -> Result<APTRepositoriesResult, Error> {
    let (config, _digest) = pdm_config::remotes::config()?;
    let remote = get_remote(&config, &remote)?;

    Ok(match remote.ty {
        RemoteType::Pve => {
            let client = connection::make_pve_client(remote)?;
            client.get_apt_repositories(&node).await?
        }
        RemoteType::Pbs => {
            let client = connection::make_pbs_client(remote)?;
            client.get_apt_repositories().await?
        }
    })
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
                type: HistoryListOptions,
                flatten: true,
            },
        },
    },
    returns: {
        description: "The APT transactions, newest first.",
        type: Array,
        items: {
            type: HistoryEntry,
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "node", "{node}", "system"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// List the APT transactions of a remote node.
async fn apt_history(
    remote: String,
    node: String,
    options: HistoryListOptions,
) -> Result<Vec<HistoryEntry>, Error> {
    let (config, _digest) = pdm_config::remotes::config()?;
    let remote = get_remote(&config, &remote)?;

    remote_updates::list_history(remote, &node, options).await
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
            "start-time": {
                description: "Start time of the transaction, as listed in the APT history.",
            },
            "start-index": {
                description: "Start index of the transaction, as listed in the APT history.",
                optional: true,
                default: 0,
            },
            format: {
                type: TermLogFormat,
                optional: true,
            },
        },
    },
    returns: {
        description: "The terminal output.",
        type: String,
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "node", "{node}", "system"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get the terminal output of an APT transaction of a remote node.
async fn apt_term_log(
    remote: String,
    node: String,
    start_time: i64,
    start_index: Option<u32>,
    format: Option<TermLogFormat>,
) -> Result<String, Error> {
    let (config, _digest) = pdm_config::remotes::config()?;
    let remote = get_remote(&config, &remote)?;

    remote_updates::get_term_log(remote, &node, start_time, start_index, format).await
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
            "start-time": {
                description: "Start time of the transaction, as listed in the APT history.",
            },
            "start-index": {
                description: "Start index of the transaction, as listed in the APT history.",
                optional: true,
                default: 0,
            },
        },
    },
    returns: {
        description: "The changelog entries, grouped by source package.",
        type: Array,
        items: {
            type: TransactionChangelog,
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "node", "{node}", "system"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get the changelog entries for the package changes of an APT transaction of a remote node.
async fn apt_transaction_changelogs(
    remote: String,
    node: String,
    start_time: i64,
    start_index: Option<u32>,
) -> Result<Vec<TransactionChangelog>, Error> {
    let (config, _digest) = pdm_config::remotes::config()?;
    let remote = get_remote(&config, &remote)?;

    remote_updates::get_transaction_changelogs(remote, &node, start_time, start_index).await
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
            name: {
                description: "Name of the installed package.",
                type: String,
                max_length: 128,
            },
            "since-version": {
                description: "Only list the entries of versions later than this one.",
                type: String,
                max_length: 128,
                optional: true,
            },
            "to-version": {
                description: "Only list the entries of versions up to including this one.",
                type: String,
                max_length: 128,
                optional: true,
            },
        },
    },
    returns: {
        description: "The changelog entries, usually newest first.",
        type: Array,
        items: {
            type: ChangelogEntry,
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "node", "{node}", "system"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get the Debian changelog of a package installed on a remote node.
async fn apt_installed_changelog(
    remote: String,
    node: String,
    name: String,
    since_version: Option<String>,
    to_version: Option<String>,
) -> Result<Vec<ChangelogEntry>, Error> {
    let (config, _digest) = pdm_config::remotes::config()?;
    let remote = get_remote(&config, &remote)?;

    remote_updates::get_installed_changelog(remote, &node, name, since_version, to_version).await
}

#[sortable]
const APT_SUBDIRS: SubdirMap = &sorted!([
    (
        "changelog",
        &Router::new().get(&API_METHOD_APT_GET_CHANGELOG),
    ),
    ("history", &Router::new().get(&API_METHOD_APT_HISTORY)),
    (
        "installed-changelog",
        &Router::new().get(&API_METHOD_APT_INSTALLED_CHANGELOG),
    ),
    (
        "repositories",
        &Router::new().get(&API_METHOD_GET_APT_REPOSITORIES),
    ),
    ("termlog", &Router::new().get(&API_METHOD_APT_TERM_LOG)),
    (
        "transaction-changelogs",
        &Router::new().get(&API_METHOD_APT_TRANSACTION_CHANGELOGS),
    ),
    (
        "update",
        &Router::new()
            .get(&API_METHOD_APT_UPDATE_AVAILABLE)
            .post(&API_METHOD_APT_UPDATE_DATABASE),
    ),
    (
        "update-changelogs",
        &Router::new().get(&API_METHOD_APT_UPDATE_CHANGELOGS),
    ),
]);

pub const APT_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(APT_SUBDIRS))
    .subdirs(APT_SUBDIRS);
