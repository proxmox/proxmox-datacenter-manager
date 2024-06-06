//! Manage remote configuration.

use anyhow::{bail, Error};

use proxmox_router::{
    http_bail, http_err, list_subdirs_api_method, Router, RpcEnvironment, SubdirMap,
};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use pdm_api_types::remotes::{PveRemoteUpdater, Remote, REMOTE_ID_SCHEMA};
use pdm_api_types::ConfigDigest;
use pdm_config::section_config::SectionConfigData;

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
    let (remotes, digest) = pdm_config::remotes::config()?;

    rpcenv["digest"] = digest.to_hex().into();

    Ok(remotes.into_iter().map(|(_id, value)| value).collect())
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
)]
/// List all the remotes this instance is managing.
pub fn add_remote(entry: Remote) -> Result<(), Error> {
    let (mut remotes, _) = pdm_config::remotes::config()?;

    let id = entry.id().to_owned();
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
                type: PveRemoteUpdater,
            },
            digest: {
                optional: true,
                type: ConfigDigest,
            },
        },
    },
)]
/// List all the remotes this instance is managing.
pub fn update_remote(
    id: String,
    updater: PveRemoteUpdater,
    digest: Option<ConfigDigest>,
) -> Result<(), Error> {
    let (mut remotes, config_digest) = pdm_config::remotes::config()?;
    config_digest.detect_modification(digest.as_ref())?;

    let entry = remotes
        .get_mut(&id)
        .ok_or_else(|| http_err!(NOT_FOUND, "no such remote {id:?}"))?;

    match entry {
        Remote::Pve(entry) => {
            if let Some(v) = updater.nodes {
                entry.nodes = v;
            }
            if let Some(v) = updater.userid {
                entry.userid = v;
            }
            if let Some(v) = updater.token {
                entry.token = v;
            }
        }
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
)]
/// Query the remote's version.
///
/// FIXME: Should we add an option to explicitly query the entire cluster to get a full version
/// overview?
pub async fn version(id: String) -> Result<pve_api_types::VersionResponse, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    match get_remote(&remotes, &id)? {
        Remote::Pve(pve) => Ok(pve::connect(pve)?.version().await?),
    }
}
