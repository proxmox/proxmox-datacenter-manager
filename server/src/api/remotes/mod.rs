//! Manage remote configuration.

use std::collections::HashSet;
use std::error::Error as _;
use std::future::Future;

use anyhow::{Context, Error, bail, format_err};
use pdm_config::remotes::RemoteConfig;
use serde::{Deserialize, Serialize};

use proxmox_access_control::CachedUserInfo;
use proxmox_router::{
    Permission, Router, RpcEnvironment, SubdirMap, http_bail, http_err, list_subdirs_api_method,
};
use proxmox_rrd_api_types::RrdMode;
use proxmox_rrd_api_types::RrdTimeframe;
use proxmox_schema::Schema;
use proxmox_schema::api;
use proxmox_section_config::typed::SectionConfigData;
use proxmox_sortable_macro::sortable;
use proxmox_time::{epoch_i64, epoch_to_rfc2822};

use pbs_api_types::{Tokenname, Userid};
use pdm_api_types::remotes::{
    REMOTE_ID_SCHEMA, Remote, RemoteType, RemoteUpdater, TlsProbeOutcome,
};
use pdm_api_types::rrddata::RemoteDatapoint;
use pdm_api_types::{Authid, ConfigDigest, PRIV_RESOURCE_AUDIT, PRIV_RESOURCE_MODIFY};

use crate::metric_collection::trigger_remote_metric_collection;
use crate::{connection, pbs_client};

use super::pve;
use super::rrd_common;
use super::rrd_common::DataPoint;

pub(crate) mod metric_collection;
pub(crate) mod shell;
pub(crate) mod tasks;
pub(crate) mod updates;

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("remote", &REMOTE_ROUTER),
    ("updates", &updates::ROUTER),
    ("tasks", &tasks::ROUTER),
    ("metric-collection", &metric_collection::ROUTER),
]);

pub const REMOTE_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_REMOTES)
    .post(&API_METHOD_ADD_REMOTE)
    .match_all("id", &ITEM_ROUTER);

const ITEM_ROUTER: Router = Router::new()
    .put(&API_METHOD_UPDATE_REMOTE)
    .delete(&API_METHOD_REMOVE_REMOTE)
    .get(&list_subdirs_api_method!(REMOTE_SUBDIRS))
    .subdirs(REMOTE_SUBDIRS);

#[sortable]
const REMOTE_SUBDIRS: SubdirMap = &sorted!([
    ("config", &Router::new().get(&API_METHOD_REMOTE_CONFIG)),
    ("version", &Router::new().get(&API_METHOD_VERSION)),
    (
        "probe-certificate",
        &Router::new().post(&API_METHOD_PROBE_REMOTE_CERTIFICATE)
    ),
    (
        "rrddata",
        &Router::new().get(&API_METHOD_GET_PER_REMOTE_RRD_DATA)
    ),
]);

pub fn get_remote<'a>(
    config: &'a SectionConfigData<Remote>,
    id: &str,
) -> Result<&'a Remote, Error> {
    config
        .get(id)
        .ok_or_else(|| http_err!(NOT_FOUND, "no such node '{id}'"))
}

/// Builder for iterating over remotes with optional filtering.
///
/// Loads the remote config once and provides chainable filter methods.
/// Implements [`IntoIterator`] so it can be used directly in `for`
/// loops or passed to iterator adapters.
///
/// # Examples
///
/// Filter by remote type:
/// ```
/// use pdm_api_types::remotes::{Remote, RemoteType};
/// use proxmox_section_config::typed::ApiSectionDataEntry;
/// use server::api::remotes::RemoteIterator;
///
/// let config = Remote::parse_section_config("test", "\
/// pve: my-pve
/// \tauthid admin@test!token
/// \tnodes 10.0.0.1
/// \ttoken secret
///
/// pbs: my-pbs
/// \tauthid admin@test!token
/// \tnodes 10.0.0.2
/// \ttoken secret
/// ").unwrap();
///
/// let names: Vec<String> = RemoteIterator::from_config(config)
///     .remote_type(RemoteType::Pve)
///     .into_names()
///     .collect();
/// assert_eq!(names, ["my-pve"]);
/// ```
///
/// With privilege filtering (requires initialized auth subsystem):
/// ```ignore
/// let remotes = RemoteIterator::new(app.remote_config())?
///     .remote_type(RemoteType::Pve)
///     .any_privs(&user_info, &auth_id, PRIV_RESOURCE_AUDIT)
///     .into_remotes();
/// fetcher.do_for_all_remotes(remotes, ...).await;
/// ```
pub struct RemoteIterator {
    remotes: Vec<(String, Remote)>,
}

impl RemoteIterator {
    /// Load the remote config and create an unfiltered iterator.
    pub fn new(config_impl: &dyn RemoteConfig) -> Result<Self, Error> {
        let (config, _) = config_impl.read()?;
        Ok(Self {
            remotes: config.into_iter().collect(),
        })
    }

    /// Create an iterator from an existing [`SectionConfigData`].
    pub fn from_config(config: SectionConfigData<Remote>) -> Self {
        Self {
            remotes: config.into_iter().collect(),
        }
    }

    /// Keep only remotes of the given type.
    pub fn remote_type(mut self, ty: RemoteType) -> Self {
        self.remotes.retain(|(_, r)| r.ty == ty);
        self
    }

    /// Keep only remotes where the user has *any* of the given privileges on `/resource/{remote}`.
    ///
    /// When multiple privilege bits are OR'd together, a remote is kept if
    /// the user has **at least one** of them.
    pub fn any_privs(mut self, user_info: &CachedUserInfo, auth_id: &Authid, privs: u64) -> Self {
        self.remotes
            .retain(|(name, _)| user_info.lookup_privs(auth_id, &["resource", name]) & privs != 0);
        self
    }

    /// Keep only remotes where the user has *all* of the given privileges on `/resource/{remote}`.
    ///
    /// When multiple privilege bits are OR'd together, a remote is kept
    /// only if the user has **every one** of them.
    pub fn all_privs(mut self, user_info: &CachedUserInfo, auth_id: &Authid, privs: u64) -> Self {
        self.remotes.retain(|(name, _)| {
            let user_privs = user_info.lookup_privs(auth_id, &["resource", name]);
            user_privs & privs == privs
        });
        self
    }

    /// Keep only remotes whose id is in the given set.
    pub fn name_filter<T: std::borrow::Borrow<str> + Eq + std::hash::Hash>(
        mut self,
        names: &HashSet<T>,
    ) -> Self {
        self.remotes.retain(|(_, r)| names.contains(r.id.as_str()));
        self
    }

    /// Consume the builder and return an iterator of just the [`Remote`] values (dropping the
    /// names).
    ///
    /// Useful when passing to [`ParallelFetcher::do_for_all_remotes`].
    pub fn into_remotes(self) -> impl Iterator<Item = Remote> {
        self.remotes.into_iter().map(|(_, r)| r)
    }

    /// Consume the builder and return an iterator of just the remote names.
    pub fn into_names(self) -> impl Iterator<Item = String> {
        self.remotes.into_iter().map(|(name, _)| name)
    }
}

/// Consuming iteration yields `(String, Remote)` pairs.
impl IntoIterator for RemoteIterator {
    type Item = (String, Remote);
    type IntoIter = std::vec::IntoIter<(String, Remote)>;

    fn into_iter(self) -> Self::IntoIter {
        self.remotes.into_iter()
    }
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
            type: Remote,
        },
    },
)]
/// List all the remotes this instance is managing.
pub fn list_remotes(rpcenv: &mut dyn RpcEnvironment) -> Result<Vec<Remote>, Error> {
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .context("no authid available")?
        .parse()?;
    let user_info = CachedUserInfo::new()?;
    let top_level_allowed = 0 != user_info.lookup_privs(&auth_id, &["resource"]);

    let (remotes, digest) = pdm_config::remotes::config()?;
    rpcenv["digest"] = digest.to_hex().into();

    Ok(remotes
        .into_iter()
        .filter_map(|(id, mut value)| {
            // FIXME: proper type here?
            value.token = String::new(); // remove secret from api response
            (top_level_allowed || 0 != user_info.lookup_privs(&auth_id, &["resource", &id]))
                .then_some(value)
        })
        .collect())
}

const CREATE_TOKEN_SCHEMA: Schema = pdm_api_types::PROXMOX_TOKEN_NAME_SCHEMA
    .unwrap_string_schema_cloned()
    .description("If given, create this token on the remote and use it.")
    .schema();

// FIXME: need to have a type spanning all remote types here... SOMEHOW... (eg. oneOf support)
#[api(
    input: {
        properties: {
            entry: {
                flatten: true,
                type: Remote,
            },
            "create-token": {
                optional: true,
                schema: CREATE_TOKEN_SCHEMA,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource"], PRIV_RESOURCE_MODIFY, false),
    },
)]
/// Add a new remote to manage.
///
/// If `create-token` is specified, a new API token is generated on the target
/// remote with the given name and used instead of the existing authentication
/// details in the entry.
///
/// Creating the token times out after 180 seconds. Waiting for the configuration lock and saving
/// the new entry do not count toward this limit.
pub async fn add_remote(mut entry: Remote, create_token: Option<String>) -> Result<(), Error> {
    // Token creation is a network round trip, so keep the global config lock out of that window
    // and only serialize mutations of this one remote for its duration.
    let _remote_lock = pdm_config::remotes::lock_remote(&entry.id)?;

    {
        let _lock = pdm_config::remotes::lock_config()?;
        let (remotes, _) = pdm_config::remotes::config()?;
        if remotes.contains_key(&entry.id) {
            bail!("entry {:?} already exists", entry.id);
        }
    }

    let created_token = match create_token {
        Some(token_name) => {
            let endpoint = entry
                .nodes
                .first()
                .map(|node| node.hostname.clone())
                .unwrap_or_default();
            create_token_with_timeout(&endpoint, create_remote_token(&mut entry, token_name))
                .await?;
            true
        }
        None => false,
    };

    let name = entry.id.clone();
    let is_pve = entry.ty == RemoteType::Pve;

    if let Err(err) = insert_remote(entry.clone()) {
        // The per-remote lock excludes concurrent adds, so only an out-of-band edit or a failed
        // save gets here, but the token created for this attempt must not be orphaned either way.
        if created_token {
            if let Err(cleanup_err) = delete_remote_token(&entry).await {
                return Err(format_err!(
                    "{err} - deleting the new token could not be confirmed ({cleanup_err}), check \
                     whether the token exists on the remote before retrying"
                ));
            }
        }
        return Err(err);
    }

    if let Err(e) = trigger_remote_metric_collection(Some(name.clone()), false).await {
        log::error!("could not trigger metric collection after adding remote: {e}");
    }

    // Fire-and-forget Ceph auto-detection for the new PVE remote so the registry
    // picks up any Ceph cluster it backs. A periodic sweep is a later refinement.
    if is_pve {
        tokio::spawn(async move {
            if let Err(e) = crate::ceph::sweep::detect_and_upsert(std::slice::from_ref(&name)).await
            {
                log::error!("ceph auto-detection after adding remote {name} failed: {e}");
            }
        });
    }

    Ok(())
}

/// Insert a new remote, holding the global config lock only for the load and save.
fn insert_remote(entry: Remote) -> Result<(), Error> {
    let _lock = pdm_config::remotes::lock_config()?;
    let (mut remotes, _) = pdm_config::remotes::config()?;

    if remotes.contains_key(&entry.id) {
        bail!("entry {:?} already exists", entry.id);
    }

    remotes.insert(entry.id.clone(), entry);
    pdm_config::remotes::save_config(remotes)
}

/// Token creation gets the onboarding deadline too. It cannot be retried blindly, so an expired
/// deadline is reported as an uncertain outcome.
async fn create_token_with_timeout<T>(
    endpoint: &str,
    future: impl Future<Output = Result<T, Error>>,
) -> Result<T, Error> {
    match tokio::time::timeout(connection::ONBOARDING_TIMEOUT, future).await {
        Ok(result) => result,
        Err(_) => {
            let secs = connection::ONBOARDING_TIMEOUT.as_secs();
            http_bail!(
                GATEWAY_TIMEOUT,
                "creating the API token on '{endpoint}' timed out after {secs} seconds - check whether the token exists on the remote before retrying"
            )
        }
    }
}

/// Shorten the `Client` error message and warn when a token may have been left behind.
///
/// Only a connection failure proves that the request never reached the remote. 401 and 403 count as
/// refusals, since stock PVE and PBS check permissions before creating the token. Any other error
/// can occur after the token was stored, for example a plain 400 from the proxy when the privileged
/// daemon fails after saving it, so the message warns against a blind retry.
fn short_create_err(err: proxmox_client::Error) -> Error {
    use http::StatusCode;

    let detail = err.source().unwrap_or(&err).to_string();
    match err {
        proxmox_client::Error::Connect(_) => format_err!("error creating token: {detail}"),
        proxmox_client::Error::Unauthorized
        | proxmox_client::Error::Api(StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN, _) => {
            format_err!("error creating token: {detail}")
        }
        _ => format_err!(
            "error creating token: {detail} - check whether the token exists on the remote before \
             retrying"
        ),
    }
}

/// The remote answered with a token ID PDM cannot use, so the token exists but cannot be adopted.
fn unusable_token_id_err(err: impl std::fmt::Display, cleaned_up: bool) -> Error {
    if cleaned_up {
        format_err!(
            "error creating token: the remote returned an unusable token ID ({err}), the new \
             token was deleted"
        )
    } else {
        format_err!(
            "error creating token: the remote returned an unusable token ID ({err}), and deleting \
             the new token could not be confirmed - check whether the token exists on the remote \
             before retrying"
        )
    }
}

/// Create an API token on the remote and switch the entry's authentication to it.
async fn create_remote_token(entry: &mut Remote, token_name: String) -> Result<(), Error> {
    let nodename = proxmox_sys::nodename();
    let date = epoch_to_rfc2822(epoch_i64())?;
    let comment = Some(format!("auto-generated by PDM host '{nodename}' on {date}"));

    let (authid, token) = match entry.ty {
        RemoteType::Pve => {
            let client = pve::connect_or_login(entry).await?;
            create_pve_token(&*client, entry.authid.user(), &token_name, comment).await?
        }
        RemoteType::Pbs => {
            let client = pbs_client::connect_or_login(entry).await?;
            let tokenname = Tokenname::try_from(token_name)?;
            create_pbs_token(&client, entry.authid.user(), &tokenname, comment).await?
        }
    };
    entry.authid = authid;
    entry.token = token;
    Ok(())
}

/// Create an API token on a Proxmox VE remote, deleting one again whose returned ID is unusable.
async fn create_pve_token(
    client: &connection::PveClient,
    user: &Userid,
    token_name: &str,
    comment: Option<String>,
) -> Result<(Authid, String), Error> {
    let token = client
        .create_token(
            user.as_str(),
            token_name,
            pve_api_types::CreateToken {
                comment,
                expire: None,
                privsep: Some(false),
            },
        )
        .await
        .map_err(short_create_err)?;

    match token.full_tokenid.parse() {
        Ok(authid) => Ok((authid, token.value)),
        Err(err) => {
            let cleanup = client.delete_token(user.as_str(), token_name).await;
            Err(unusable_token_id_err(err, cleanup.is_ok()))
        }
    }
}

/// Create an admin API token on a Proxmox Backup Server remote, deleting one again whose returned
/// ID is unusable.
async fn create_pbs_token<C>(
    client: &pbs_client::PbsClient<C>,
    userid: &Userid,
    tokenname: &Tokenname,
    comment: Option<String>,
) -> Result<(Authid, String), Error>
where
    C: proxmox_client::HttpApiClient<Body = proxmox_http::Body>,
{
    let token = client
        .create_admin_token(
            userid.clone(),
            tokenname.clone(),
            pbs_client::CreateToken {
                comment,
                enable: Some(true),
                expire: None,
            },
        )
        .await
        .map_err(short_create_err)?;

    match token.tokenid.parse() {
        Ok(authid) => Ok((authid, token.value)),
        Err(err) => {
            let cleanup = client.delete_token(userid, tokenname).await;
            Err(unusable_token_id_err(err, cleanup.is_ok()))
        }
    }
}

/// Delete the API token the remote entry authenticates with on the remote itself.
async fn delete_remote_token(remote: &Remote) -> Result<(), Error> {
    let user = remote.authid.user();

    let token_name = remote
        .authid
        .tokenname()
        .ok_or_else(|| format_err!("Unable to find the token for the remote {:?}", remote.id))?;

    // With the `Client`'s error type the message gets a bit long, shorten it:
    let short_delete_err = |err: proxmox_client::Error| {
        format_err!("error deleting token: {}", err.source().unwrap_or(&err))
    };

    match remote.ty {
        RemoteType::Pve => connection::make_pve_client(remote)?
            .delete_token(user.as_str(), token_name.as_str())
            .await
            .map_err(short_delete_err),
        RemoteType::Pbs => connection::make_pbs_client(remote)?
            .delete_token(user, token_name)
            .await
            .map_err(short_delete_err),
    }
}

#[api()]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
/// Deletable property name
pub enum DeletableProperty {
    /// Delete the web-url property.
    WebUrl,
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
            delete: {
                description: "List of properties to delete.",
                type: Array,
                optional: true,
                items: {
                    type: DeletableProperty,
                }
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
/// Update an existing managed remote.
pub fn update_remote(
    id: String,
    updater: RemoteUpdater,
    delete: Option<Vec<DeletableProperty>>,
    digest: Option<ConfigDigest>,
) -> Result<(), Error> {
    let _remote_lock = pdm_config::remotes::lock_remote(&id)?;
    let _lock = pdm_config::remotes::lock_config()?;
    let (mut remotes, config_digest) = pdm_config::remotes::config()?;
    config_digest.detect_modification(digest.as_ref())?;

    let entry = remotes
        .get_mut(&id)
        .ok_or_else(|| http_err!(NOT_FOUND, "no such remote {id:?}"))?;

    if let Some(delete) = delete {
        for delete_prop in delete {
            match delete_prop {
                DeletableProperty::WebUrl => {
                    entry.web_url = None;
                }
            }
        }
    }

    if let Some(v) = updater.nodes {
        entry.nodes = v;
    }
    if let Some(v) = updater.authid {
        entry.authid = v;
    }
    if let Some(v) = updater.token {
        entry.token = v;
    }

    if updater.web_url.is_some() {
        entry.web_url = updater.web_url;
    }

    pdm_config::remotes::save_config(remotes)?;

    Ok(())
}

#[api(
    input: {
        properties: {
            id: { schema: REMOTE_ID_SCHEMA },
            node: {
                type: String,
                description: "Hostname of the configured node to probe.",
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{id}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Re-probe a configured node's TLS certificate without using the pinned fingerprint.
///
/// Lets clients detect a rotated certificate (the live connection only fails with an
/// ambiguous connect error) and decide whether to update the stored fingerprint. This
/// never changes the stored pin; updating it goes through `update_remote` after explicit
/// user confirmation.
pub async fn probe_remote_certificate(id: String, node: String) -> Result<TlsProbeOutcome, Error> {
    let endpoint = node.clone();
    connection::run_remote_operation(&endpoint, async move {
        let (remotes, _) = pdm_config::remotes::config()?;
        let remote = get_remote(&remotes, &id)?;

        let node_url = remote
            .nodes
            .iter()
            .find(|n| n.hostname == node)
            .ok_or_else(|| {
                http_err!(NOT_FOUND, "node {node:?} not configured for remote {id:?}")
            })?;

        connection::probe_tls_connection(remote.ty, node_url.hostname.clone(), None).await
    })
    .await
}

#[api(
    input: {
        properties: {
            id: { schema: REMOTE_ID_SCHEMA },
            "delete-token": {
                optional: true,
                default: false,
                description: "If true, deletes the API token on the remote.",
            }
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource"], PRIV_RESOURCE_MODIFY, false),
    },
)]
/// Remove a remote that this instance is managing.
pub async fn remove_remote(id: String, delete_token: bool) -> Result<(), Error> {
    let _remote_lock = pdm_config::remotes::lock_remote(&id)?;

    if delete_token {
        let remote = {
            let _lock = pdm_config::remotes::lock_config()?;
            let (remotes, _) = pdm_config::remotes::config()?;
            remotes
                .get(&id)
                .ok_or_else(|| http_err!(NOT_FOUND, "no such remote {id:?}"))?
                .clone()
        };

        delete_remote_token(&remote).await?;
    }

    let removed = {
        let _lock = pdm_config::remotes::lock_config()?;
        let (mut remotes, _) = pdm_config::remotes::config()?;
        let removed = remotes.remove(&id).is_some();
        if removed {
            pdm_config::remotes::save_config(remotes)?;
        }
        removed
    };

    if !removed {
        if delete_token {
            // Only an out-of-band edit can remove the entry underneath the per-remote lock, and
            // with the token already deleted the requested end state is reached.
            log::warn!("remote {id:?} vanished while its token was being deleted");
        } else {
            http_bail!(NOT_FOUND, "no such entry {id:?}");
        }
    }

    // Drop any Ceph members that referenced this remote (and clusters left
    // empty). Best-effort: the remote is already gone, so a cleanup failure
    // must not fail the removal.
    if let Err(e) = crate::ceph::sweep::on_remote_removed(&id) {
        log::error!("failed to clean up ceph registry after removing remote {id}: {e}");
    }

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
        RemoteType::Pve => Ok(connection::make_pve_client(remote)?.version().await?),
        RemoteType::Pbs => Ok(connection::make_pbs_client(remote)?.version().await?),
    }
}

#[api(
    input: {
        properties: {
            id: { schema: REMOTE_ID_SCHEMA },
        },
    },
    returns: { type: Remote },
    access: {
        permission: &Permission::Privilege(&["resource", "{id}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get the Remote Configuration
pub fn remote_config(id: String) -> Result<Remote, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let mut remote = get_remote(&remotes, &id)?.clone();
    // FIXME: proper type here?
    remote.token = String::new(); // mask token in response
    Ok(remote.clone())
}

impl DataPoint for RemoteDatapoint {
    fn new(time: u64) -> Self {
        Self {
            time,
            ..Default::default()
        }
    }

    fn fields() -> &'static [&'static str] {
        &["metric-collection-response-time"]
    }

    fn set_field(&mut self, name: &str, value: f64) {
        if name == "metric-collection-response-time" {
            self.metric_collection_response_time = Some(value);
        }
    }
}

#[api(
    input: {
        properties: {
            id: {
                schema: REMOTE_ID_SCHEMA,
            },
            timeframe: {
                type: RrdTimeframe,
            },
            cf: {
                type: RrdMode,
            },
        },
    },
    returns: {
        type: Array,
        description: "A list remote RRD data points.",
        items: {
            type: RemoteDatapoint,
        }
    }
)]
/// Read metric collection RRD data.
fn get_per_remote_rrd_data(
    id: String,
    timeframe: RrdTimeframe,
    cf: RrdMode,
) -> Result<Vec<RemoteDatapoint>, Error> {
    let base = format!("remotes/{id}");
    rrd_common::create_datapoints_from_rrd(&base, timeframe, cf)
}

#[cfg(test)]
mod tests {
    use std::collections::{HashSet, VecDeque};
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::Duration;

    use anyhow::Error;
    use futures::future::{Pending, Ready, pending, ready};
    use http::{Method, StatusCode};
    use pdm_api_types::ConfigDigest;
    use pdm_api_types::remotes::{Remote, RemoteType, RemoteUpdater};
    use pdm_config::remotes::RemoteConfig;
    use proxmox_client::{
        Error as ClientError, HttpApiClient, HttpApiResponse, HttpApiResponseStream,
    };
    use proxmox_product_config::{ApiLockGuard, open_api_lockfile};
    use proxmox_router::HttpError;
    use proxmox_section_config::typed::{ApiSectionDataEntry, SectionConfigData};
    use serde::Serialize;

    use super::{
        RemoteIterator, create_pbs_token, create_pve_token, create_token_with_timeout,
        short_create_err, update_remote,
    };
    use crate::pbs_client::{CreateToken, PbsClient};
    use pve_api_types::client::PveClientImpl;

    /// Parse a `remotes.cfg`-style string into a [`SectionConfigData`].
    fn parse_config(raw: &str) -> SectionConfigData<Remote> {
        Remote::parse_section_config("test", raw).expect("valid test config")
    }

    const TEST_CONFIG: &str = "\
pve: pve-prod
\tauthid admin@test!token
\tnodes 10.0.0.1
\ttoken secret

pve: pve-staging
\tauthid admin@test!token
\tnodes 10.0.0.2
\ttoken secret

pbs: pbs-backup
\tauthid backup@test!token
\tnodes 10.0.0.3
\ttoken secret

pbs: pbs-offsite
\tauthid backup@test!token
\tnodes 10.0.0.4
\ttoken secret
";

    /// In-memory config with real file locks to exercise update exclusion.
    struct TestRemoteConfig {
        config: Mutex<SectionConfigData<Remote>>,
        lock_dir: PathBuf,
    }

    impl TestRemoteConfig {
        const LOCK_TIMEOUT: Duration = Duration::from_millis(250);
    }

    impl RemoteConfig for TestRemoteConfig {
        fn read(&self) -> Result<(SectionConfigData<Remote>, ConfigDigest), Error> {
            Ok((self.config.lock().unwrap().clone(), [0u8; 32].into()))
        }

        fn read_secret_token(&self, remote: &Remote) -> Result<String, Error> {
            Ok(remote.token.clone())
        }

        fn lock(&self) -> Result<ApiLockGuard, Error> {
            let path = self.lock_dir.join(".remotes.lock");
            open_api_lockfile(path, Some(Self::LOCK_TIMEOUT), true)
        }

        fn lock_remote(&self, id: &str) -> Result<ApiLockGuard, Error> {
            let path = self.lock_dir.join(format!(".remote-{id}.lock"));
            open_api_lockfile(path, Some(Self::LOCK_TIMEOUT), true)
        }

        fn write(&self, remotes: SectionConfigData<Remote>) -> Result<(), Error> {
            *self.config.lock().unwrap() = remotes;
            Ok(())
        }
    }

    fn configured_token(id: &str) -> String {
        let (remotes, _) = pdm_config::remotes::config().unwrap();
        remotes.get(id).unwrap().token.clone()
    }

    fn token_update(token: &str) -> RemoteUpdater {
        RemoteUpdater {
            token: Some(token.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn update_respects_per_remote_lock() {
        crate::test_support::init_product_config();
        let lock_dir = tempfile::tempdir().unwrap();
        pdm_config::remotes::init(Box::new(TestRemoteConfig {
            config: Mutex::new(parse_config(TEST_CONFIG)),
            lock_dir: lock_dir.path().to_path_buf(),
        }));

        let remote_lock = pdm_config::remotes::lock_remote("pve-prod").unwrap();

        update_remote(
            "pve-staging".to_string(),
            token_update("staging-updated"),
            None,
            None,
        )
        .expect("unrelated remote must remain writable");
        assert_eq!(configured_token("pve-staging"), "staging-updated");

        update_remote(
            "pve-prod".to_string(),
            token_update("prod-updated"),
            None,
            None,
        )
        .expect_err("same remote must remain locked");
        assert_eq!(configured_token("pve-prod"), "secret");

        drop(remote_lock);
        update_remote(
            "pve-prod".to_string(),
            token_update("prod-updated"),
            None,
            None,
        )
        .unwrap();
        assert_eq!(configured_token("pve-prod"), "prod-updated");
    }

    #[tokio::test(start_paused = true)]
    async fn token_creation_timeout_reports_uncertain_outcome() {
        let error = create_token_with_timeout(
            "pve.example.test:8006",
            std::future::pending::<Result<(), Error>>(),
        )
        .await
        .expect_err("pending token creation did not time out");
        let error = error
            .downcast::<HttpError>()
            .expect("timeout did not produce an HTTP error");

        assert_eq!(error.code, StatusCode::GATEWAY_TIMEOUT);
        assert!(error.message.contains("'pve.example.test:8006'"));
        assert!(error.message.contains("check whether the token exists"));
    }

    #[test]
    fn token_creation_errors_flag_uncertain_outcomes() {
        assert_eq!(
            short_create_err(ClientError::Unauthorized).to_string(),
            "error creating token: unauthorized"
        );
        for (error, uncertain) in [
            (ClientError::Connect("detail".into()), false),
            (
                ClientError::Api(StatusCode::UNAUTHORIZED, "detail".into()),
                false,
            ),
            (
                ClientError::Api(StatusCode::FORBIDDEN, "detail".into()),
                false,
            ),
            (
                ClientError::Api(StatusCode::BAD_REQUEST, "detail".into()),
                true,
            ),
            (
                ClientError::Api(StatusCode::BAD_GATEWAY, "detail".into()),
                true,
            ),
            (ClientError::Client("detail".into()), true),
            (ClientError::BadApi("detail".into(), None), true),
            (ClientError::Anyhow(anyhow::anyhow!("detail")), true),
        ] {
            let message = short_create_err(error).to_string();
            assert!(message.contains("detail"));
            assert_eq!(
                message.contains("check whether the token exists"),
                uncertain,
                "{message}"
            );
        }
    }

    /// Answers requests with scripted responses and records what was asked.
    struct ScriptedClient {
        requests: Mutex<Vec<(Method, String)>>,
        responses: Mutex<VecDeque<Result<HttpApiResponse, ClientError>>>,
    }

    impl ScriptedClient {
        fn new(responses: Vec<Result<HttpApiResponse, ClientError>>) -> Self {
            Self {
                requests: Mutex::new(Vec::new()),
                responses: Mutex::new(responses.into()),
            }
        }

        fn requests(&self) -> Vec<(Method, String)> {
            self.requests.lock().unwrap().clone()
        }
    }

    impl HttpApiClient for ScriptedClient {
        type ResponseFuture<'a>
            = Ready<Result<HttpApiResponse, ClientError>>
        where
            Self: 'a;
        type Body = proxmox_http::Body;
        type ResponseStreamFuture<'a>
            = Pending<Result<HttpApiResponseStream<Self::Body>, ClientError>>
        where
            Self: 'a;

        fn request<'a, T>(
            &'a self,
            method: Method,
            path_and_query: &'a str,
            _params: Option<T>,
        ) -> Self::ResponseFuture<'a>
        where
            T: Serialize + 'a,
        {
            self.requests
                .lock()
                .unwrap()
                .push((method, path_and_query.to_string()));
            ready(
                self.responses
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("unexpected API request"),
            )
        }

        fn streaming_request<'a, T>(
            &'a self,
            _method: Method,
            _path_and_query: &'a str,
            _params: Option<T>,
        ) -> Self::ResponseStreamFuture<'a>
        where
            T: Serialize + 'a,
        {
            pending()
        }
    }

    fn extjs_response(body: &str) -> HttpApiResponse {
        HttpApiResponse {
            status: 200,
            content_type: Some("application/json".to_string()),
            body: body.as_bytes().to_vec(),
        }
    }

    #[tokio::test]
    async fn pbs_acl_rejection_reports_cleanup_outcome() {
        for cleanup_succeeds in [true, false] {
            let cleanup = if cleanup_succeeds {
                Ok(extjs_response(r#"{"data":null,"success":true}"#))
            } else {
                Err(deletion_failure())
            };
            let client = PbsClient(ScriptedClient::new(vec![
                Ok(extjs_response(
                    r#"{"data":{"tokenid":"root@pam!pdm","value":"created-secret"},"success":true}"#,
                )),
                Ok(extjs_response(
                    r#"{"success":false,"status":403,"message":"no permission","data":null}"#,
                )),
                cleanup,
            ]));

            let error = client
                .create_admin_token(
                    "root@pam".parse().unwrap(),
                    pbs_api_types::Tokenname::try_from("pdm".to_string()).unwrap(),
                    CreateToken {
                        comment: None,
                        enable: Some(true),
                        expire: None,
                    },
                )
                .await
                .expect_err("rejected ACL must fail token creation");

            if cleanup_succeeds {
                assert!(matches!(error, ClientError::Api(StatusCode::FORBIDDEN, _)));
            } else {
                let message = short_create_err(error).to_string();
                assert!(message.contains("no permission"));
                assert!(message.contains("deletion failed"));
                assert!(message.contains("could not be confirmed"));
                assert_eq!(message.matches("before retrying").count(), 1);
            }
            assert_eq!(
                client.0.requests(),
                [
                    (
                        Method::POST,
                        "/api2/extjs/access/users/root@pam/token/pdm".to_string()
                    ),
                    (Method::PUT, "/api2/extjs/access/acl".to_string()),
                    (
                        Method::DELETE,
                        "/api2/extjs/access/users/root%40pam/token/pdm".to_string()
                    ),
                ]
            );
        }
    }

    const PVE_USER_TOKEN_PATH: &str = "/api2/extjs/access/users/root%40pam/token/pdm";

    fn deletion_failure() -> ClientError {
        ClientError::Api(
            StatusCode::INTERNAL_SERVER_ERROR,
            "deletion failed".to_string(),
        )
    }

    #[tokio::test]
    async fn pve_unusable_token_id_deletes_the_token_again() {
        for (deletion, wording) in [
            (
                Ok(extjs_response(r#"{"data":null,"success":true}"#)),
                "token was deleted",
            ),
            (Err(deletion_failure()), "could not be confirmed"),
        ] {
            let client = PveClientImpl(ScriptedClient::new(vec![
                Ok(extjs_response(
                    r#"{"data":{"full-tokenid":"garbage","info":{},"value":"secret"},"success":true}"#,
                )),
                deletion,
            ]));

            let error = create_pve_token(&client, &"root@pam".parse().unwrap(), "pdm", None)
                .await
                .expect_err("unusable token ID was accepted");

            assert!(error.to_string().contains("unusable token ID"));
            assert!(error.to_string().contains(wording));
            assert_eq!(
                client.0.requests(),
                [
                    (Method::POST, PVE_USER_TOKEN_PATH.to_string()),
                    (Method::DELETE, PVE_USER_TOKEN_PATH.to_string()),
                ]
            );
        }
    }

    #[tokio::test]
    async fn pbs_unusable_token_id_deletes_the_token_again() {
        for (deletion, wording) in [
            (
                Ok(extjs_response(r#"{"data":null,"success":true}"#)),
                "token was deleted",
            ),
            (Err(deletion_failure()), "could not be confirmed"),
        ] {
            let client = PbsClient(ScriptedClient::new(vec![
                Ok(extjs_response(
                    r#"{"data":{"tokenid":"garbage","value":"secret"},"success":true}"#,
                )),
                Ok(extjs_response(r#"{"data":null,"success":true}"#)),
                deletion,
            ]));

            let error = create_pbs_token(
                &client,
                &"root@pam".parse().unwrap(),
                &pbs_api_types::Tokenname::try_from("pdm".to_string()).unwrap(),
                None,
            )
            .await
            .expect_err("unusable token ID was accepted");

            assert!(error.to_string().contains("unusable token ID"));
            assert!(error.to_string().contains(wording));
            assert_eq!(
                client.0.requests(),
                [
                    (
                        Method::POST,
                        "/api2/extjs/access/users/root@pam/token/pdm".to_string()
                    ),
                    (Method::PUT, "/api2/extjs/access/acl".to_string()),
                    (
                        Method::DELETE,
                        "/api2/extjs/access/users/root%40pam/token/pdm".to_string()
                    ),
                ]
            );
        }
    }

    #[test]
    fn type_and_name_filters_intersect() {
        let filter: HashSet<&str> = ["pve-prod", "pve-enoent"].into_iter().collect();

        let names: Vec<String> = RemoteIterator::from_config(parse_config(TEST_CONFIG))
            .remote_type(RemoteType::Pve)
            .name_filter(&filter)
            .into_names()
            .collect();

        assert_eq!(names, ["pve-prod"]);
    }

    #[test]
    fn into_remotes_preserves_parsed_fields() {
        let remotes: Vec<Remote> = RemoteIterator::from_config(parse_config(TEST_CONFIG))
            .remote_type(RemoteType::Pbs)
            .into_remotes()
            .collect();

        assert_eq!(remotes.len(), 2);
        assert_eq!(remotes[0].id, "pbs-backup");
        assert_eq!(remotes[0].authid.to_string(), "backup@test!token");
        assert_eq!(remotes[1].id, "pbs-offsite");
    }

    #[test]
    fn empty_config() {
        let count = RemoteIterator::from_config(SectionConfigData::default())
            .remote_type(RemoteType::Pve)
            .into_names()
            .count();

        assert_eq!(count, 0);
    }
}
