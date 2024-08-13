//! Manage remote configuration.

use anyhow::{bail, format_err, Error};

use proxmox_access_control::CachedUserInfo;
use proxmox_router::{
    http_bail, http_err, list_subdirs_api_method, Permission, Router, RpcEnvironment, SubdirMap,
};
use proxmox_schema::api;
use proxmox_section_config::typed::SectionConfigData;
use proxmox_sortable_macro::sortable;

use pdm_api_types::remotes::{Remote, RemoteType, RemoteUpdater, REMOTE_ID_SCHEMA};
use pdm_api_types::{Authid, ConfigDigest, PRIV_RESOURCE_AUDIT, PRIV_RESOURCE_MODIFY};

use super::pve;

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_REMOTES)
    .post(&API_METHOD_ADD_REMOTE)
    .match_all("id", &ITEM_ROUTER);

const ITEM_ROUTER: Router = Router::new()
    .put(&API_METHOD_UPDATE_REMOTE)
    .delete(&API_METHOD_REMOVE_REMOTE)
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([("version", &Router::new().get(&API_METHOD_VERSION)),]);

pub fn get_remote<'a>(
    config: &'a SectionConfigData<Remote>,
    id: &str,
) -> Result<&'a Remote, Error> {
    config
        .get(id)
        .ok_or_else(|| http_err!(NOT_FOUND, "no such node '{id}'"))
}

#[api(
    access: {
        permission: &Permission::Anybody,
        description: "Returns the resources the user has access to.",
    },
    returns: {
        description: "The list of configured remotes.",
        type: Array,
        items: {
            description: "Remote entry",
            type: Object,
            properties: {},
        },
    },
)]
/// List all the remotes this instance is managing.
pub fn list_remotes(rpcenv: &mut dyn RpcEnvironment) -> Result<Vec<Remote>, Error> {
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .ok_or_else(|| format_err!("no authid available"))?
        .parse()?;
    let user_info = CachedUserInfo::new()?;
    let top_level_allowed = 0 != user_info.lookup_privs(&auth_id, &["resource"]);

    let (remotes, digest) = pdm_config::remotes::config()?;

    rpcenv["digest"] = digest.to_hex().into();

    Ok(remotes
        .into_iter()
        .filter_map(|(id, value)| {
            (top_level_allowed || 0 != user_info.lookup_privs(&auth_id, &["resource", &id]))
                .then_some(value)
        })
        .collect())
}

// FIXME: need to have a type spanning all remote types here... SOMEHOW... (eg. oneOf support)
#[api(
    input: {
        properties: {
            entry: {
                flatten: true,
                type: Remote,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource"], PRIV_RESOURCE_MODIFY, false),
    },
)]
/// List all the remotes this instance is managing.
pub fn add_remote(entry: Remote) -> Result<(), Error> {
    let (mut remotes, _) = pdm_config::remotes::config()?;

    let id = entry.id.to_owned();
    if remotes.insert(id.clone(), entry).is_some() {
        bail!("entry {id:?} already exists");
    }

    pdm_config::remotes::save_config(&remotes)?;

    Ok(())
}

// FIXME: Support `OneOf` in schema so we can use a derived Updater for all product types?
// Otherwise we need to have a custom updater struct that covers all product remotes.
#[api(
    input: {
        properties: {
            id: { schema: REMOTE_ID_SCHEMA },
            updater: {
                flatten: true,
                type: RemoteUpdater,
            },
            digest: {
                optional: true,
                type: ConfigDigest,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{id}"], PRIV_RESOURCE_MODIFY, false),
    },
)]
/// List all the remotes this instance is managing.
pub fn update_remote(
    id: String,
    updater: RemoteUpdater,
    digest: Option<ConfigDigest>,
) -> Result<(), Error> {
    let (mut remotes, config_digest) = pdm_config::remotes::config()?;
    config_digest.detect_modification(digest.as_ref())?;

    let entry = remotes
        .get_mut(&id)
        .ok_or_else(|| http_err!(NOT_FOUND, "no such remote {id:?}"))?;

    if let Some(v) = updater.nodes {
        entry.nodes = v;
    }
    if let Some(v) = updater.authid {
        entry.authid = v;
    }
    if let Some(v) = updater.token {
        entry.token = v;
    }

    pdm_config::remotes::save_config(&remotes)?;

    Ok(())
}

#[api(
    input: {
        properties: {
            id: { schema: REMOTE_ID_SCHEMA },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource"], PRIV_RESOURCE_MODIFY, false),
    },
)]
/// List all the remotes this instance is managing.
pub fn remove_remote(id: String) -> Result<(), Error> {
    let (mut remotes, _) = pdm_config::remotes::config()?;

    if remotes.remove(&id).is_none() {
        http_bail!(NOT_FOUND, "no such entry {id:?}");
    }

    pdm_config::remotes::save_config(&remotes)?;

    Ok(())
}

#[api(
    input: {
        properties: {
            id: { schema: REMOTE_ID_SCHEMA },
        },
    },
    returns: { type: pve_api_types::VersionResponse },
    access: {
        permission: &Permission::Privilege(&["resource", "{id}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Query the remote's version.
///
/// FIXME: Should we add an option to explicitly query the entire cluster to get a full version
/// overview?
pub async fn version(id: String) -> Result<pve_api_types::VersionResponse, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let remote = get_remote(&remotes, &id)?;
    match remote.ty {
        RemoteType::Pve => Ok(pve::connect(remote)?.version().await?),
        RemoteType::Pbs => Ok(crate::pbs_client::connect(remote)?.version().await?),
    }
}
