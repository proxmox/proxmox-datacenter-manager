//! Manage remote configuration.

use anyhow::{bail, Error};
use serde_json::Value;

use proxmox_router::{http_bail, http_err, RpcEnvironment};
use proxmox_schema::api;

use pdm_api_types::{RemoteType, PROXMOX_CONFIG_DIGEST_SCHEMA, REMOTE_ID_SCHEMA};
use pdm_config::remotes::Remote;

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

#[api(
    input: {
        properties: {
            type: { type: RemoteType },
        },
    },
)]
/// List all the remotes this instance is managing.
pub fn add_remote(r#type: RemoteType, params: Value) -> Result<(), Error> {
    let entry = match r#type {
        RemoteType::Pve => Remote::Pve(serde_json::from_value(params)?),
    };

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
                type: pdm_api_types::PveRemoteUpdater,
            },
            digest: {
                optional: true,
                schema: PROXMOX_CONFIG_DIGEST_SCHEMA,
            },
        },
    },
)]
/// List all the remotes this instance is managing.
pub fn update_remote(
    id: String,
    updater: pdm_api_types::PveRemoteUpdater,
    digest: Option<String>,
) -> Result<(), Error> {
    let (mut remotes, config_digest) = pdm_config::remotes::config()?;
    pdm_config::detect_modified_configuration_file(digest.as_deref(), &config_digest)?;

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
