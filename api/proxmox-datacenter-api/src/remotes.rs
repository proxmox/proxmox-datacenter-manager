//! Manage remote configuration.

use anyhow::{bail, format_err, Error};

use proxmox_client::Environment;
use proxmox_router::{http_bail, http_err, Router, RpcEnvironment};
use proxmox_schema::api;

use pdm_api_types::{PveRemote, Remote, PROXMOX_CONFIG_DIGEST_SCHEMA, REMOTE_ID_SCHEMA};
use pdm_config::section_config::SectionConfigData;

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_REMOTES)
    .post(&API_METHOD_ADD_REMOTE)
    .match_all("id", &ITEM_ROUTER);

const ITEM_ROUTER: Router = Router::new()
    .put(&API_METHOD_UPDATE_REMOTE)
    .delete(&API_METHOD_REMOVE_REMOTE);

pub fn get_remote<'a>(
    config: &'a SectionConfigData<Remote>,
    id: &str,
) -> Result<&'a Remote, Error> {
    config
        .get(id)
        .ok_or_else(|| http_err!(NOT_FOUND, "no such node '{id}'"))
}

type PveClient = pve_client::Client<PveEnv>;

pub struct PveEnv {
    remote: PveRemote,
}

impl PveEnv {
    pub fn new(remote: PveRemote) -> Self {
        Self { remote }
    }
}

impl Environment for PveEnv {
    type Error = Error;

    fn query_userid(&self, _: &http::Uri) -> Result<String, Error> {
        Ok(self.remote.userid.clone())
    }

    fn load_ticket(&self, _: &http::Uri, _userid: &str) -> Result<Option<Vec<u8>>, Error> {
        Ok(Some(self.remote.token.as_bytes().to_vec()))
    }
}

pub fn connect(remote: &PveRemote) -> Result<PveClient, Error> {
    let node = remote
        .nodes
        .first()
        .ok_or_else(|| format_err!("no nodes configured for remote"))?;

    let mut options = pve_client::Options::new();
    if let Some(fp) = &node.fingerprint {
        options = options.tls_fingerprint_str(fp)?;
    }

    let client = PveClient::new(PveEnv::new(remote.clone()), &node.hostname, options)?;

    client.client.use_api_token(proxmox_client::Token {
        userid: remote.userid.clone(),
        prefix: "PVEAPIToken".to_string(),
        value: remote.token.to_string(),
    });

    Ok(client)
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

#[api(
    input: {
        properties: {
            id: { schema: REMOTE_ID_SCHEMA },
        },
    },
)]
/// Query the remote's version.
///
/// FIXME: Should we add an option to explicitly query the entire cluster to get a full version
/// overview?
pub async fn version(id: String) -> Result<pve_client::types::VersionResponse, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    match get_remote(&remotes, &id)? {
        Remote::Pve(pve) => connect(pve)?.version().await,
    }
}
