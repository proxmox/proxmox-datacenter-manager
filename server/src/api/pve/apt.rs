use anyhow::Error;

use proxmox_apt_api_types::{APTGetChangelogOptions, APTUpdateInfo};
use proxmox_router::{list_subdirs_api_method, Permission, Router, SubdirMap};
use proxmox_schema::api;
use proxmox_schema::api_types::NODE_SCHEMA;

use pdm_api_types::{remotes::REMOTE_ID_SCHEMA, RemoteUpid, PRIV_RESOURCE_MODIFY};

use crate::{api::remotes::get_remote, remote_updates};

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

    remote_updates::get_changelog(remote.clone(), &node, options.name).await
}

const SUBDIRS: SubdirMap = &[
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

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);
