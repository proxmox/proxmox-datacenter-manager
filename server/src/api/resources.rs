use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{LazyLock, RwLock};

use anyhow::{bail, format_err, Error};
use futures::future::join_all;
use futures::FutureExt;

use pbs_api_types::{
    DataStoreStatusListItem, DatastoreBackendConfig, DatastoreBackendType, NodeStatus,
};
use pdm_api_types::remotes::{Remote, RemoteType};
use pdm_api_types::resource::{
    FailedRemote, PbsDatastoreResource, PbsNodeResource, PveLxcResource, PveNodeResource,
    PveQemuResource, PveSdnResource, PveStorageResource, RemoteResources, Resource, ResourceType,
    ResourcesStatus, SdnStatus, SdnZoneResource, TopEntities, PBS_DATASTORE_HIGH_USAGE_THRESHOLD,
};
use pdm_api_types::subscription::{
    NodeSubscriptionInfo, RemoteSubscriptionState, RemoteSubscriptions, SubscriptionLevel,
};
use pdm_api_types::{Authid, PRIV_RESOURCE_AUDIT};
use pdm_search::{Search, SearchTerm};
use proxmox_access_control::CachedUserInfo;
use proxmox_router::{
    http_bail, list_subdirs_api_method, Permission, Router, RpcEnvironment, SubdirMap,
};
use proxmox_rrd_api_types::RrdTimeframe;
use proxmox_schema::{api, parse_boolean};
use proxmox_sortable_macro::sortable;
use proxmox_subscription::SubscriptionStatus;
use pve_api_types::{ClusterResource, ClusterResourceType};

use crate::connection;
use crate::metric_collection::top_entities;

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("list", &Router::new().get(&API_METHOD_GET_RESOURCES)),
    ("status", &Router::new().get(&API_METHOD_GET_STATUS)),
    (
        "top-entities",
        &Router::new().get(&API_METHOD_GET_TOP_ENTITIES)
    ),
    (
        "subscription",
        &Router::new().get(&API_METHOD_GET_SUBSCRIPTION_STATUS)
    ),
]);

enum MatchCategory {
    Type,
    Name,
    Id,
    Status,
    Template,
    Remote,
    RemoteType,
    Property,
}

impl std::str::FromStr for MatchCategory {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let category = match s {
            "type" => MatchCategory::Type,
            "name" => MatchCategory::Name,
            "id" => MatchCategory::Id,
            "status" => MatchCategory::Status,
            "template" => MatchCategory::Template,
            "remote" => MatchCategory::Remote,
            "remote-type" => MatchCategory::RemoteType,
            "property" => MatchCategory::Property,
            _ => bail!("invalid category"),
        };
        Ok(category)
    }
}

impl MatchCategory {
    fn matches(&self, value: &str, search_term: &str) -> bool {
        match self {
            MatchCategory::Type | MatchCategory::Status => value
                .to_lowercase()
                .starts_with(&search_term.to_lowercase()),
            MatchCategory::Name | MatchCategory::Id | MatchCategory::Remote => {
                value.to_lowercase().contains(&search_term.to_lowercase())
            }
            MatchCategory::Template => match (parse_boolean(value), parse_boolean(search_term)) {
                (Ok(a), Ok(b)) => a == b,
                _ => false,
            },
            MatchCategory::RemoteType => match (
                RemoteType::from_str(value),
                RemoteType::from_str(search_term),
            ) {
                (Ok(a), Ok(b)) => a == b,
                _ => false,
            },
            MatchCategory::Property => value
                .to_lowercase()
                .split(",")
                .any(|property| property == search_term.to_lowercase()),
        }
    }
}

// returns None if we can't decide if it matches, currently only for the `RemoteType` category
fn resource_matches_search_term(
    remote_name: &str,
    resource: &Resource,
    term: &SearchTerm,
) -> Option<bool> {
    let matches = match term.category.as_deref().map(|c| c.parse::<MatchCategory>()) {
        Some(Ok(category)) => match category {
            MatchCategory::Type => category.matches(resource.resource_type().as_str(), &term.value),
            MatchCategory::Name => category.matches(resource.name(), &term.value),
            MatchCategory::Id => category.matches(&resource.id(), &term.value),
            MatchCategory::Status => category.matches(resource.status(), &term.value),
            MatchCategory::Property => category.matches(&resource.properties(), &term.value),
            MatchCategory::Template => match resource {
                Resource::PveQemu(PveQemuResource { template, .. })
                | Resource::PveLxc(PveLxcResource { template, .. }) => {
                    category.matches(&template.to_string(), &term.value)
                }
                _ => false,
            },
            MatchCategory::Remote => category.matches(remote_name, &term.value),
            MatchCategory::RemoteType => return None,
        },
        Some(Err(_)) => false,
        None => {
            MatchCategory::Name.matches(resource.name(), &term.value)
                || MatchCategory::Id.matches(&resource.id(), &term.value)
        }
    };
    Some(matches)
}

fn remote_matches_search_term(
    remote_name: &str,
    remote: &Remote,
    online: Option<bool>,
    term: &SearchTerm,
) -> bool {
    match term.category.as_deref().map(|c| c.parse::<MatchCategory>()) {
        Some(Ok(category)) => match category {
            MatchCategory::Type => category.matches("remote", &term.value),
            MatchCategory::Name | MatchCategory::Remote | MatchCategory::Id => {
                category.matches(remote_name, &term.value)
            }
            MatchCategory::Status => match online {
                Some(true) => category.matches("online", &term.value),
                Some(false) => category.matches("offline", &term.value),
                None => true,
            },
            MatchCategory::Property => false,
            MatchCategory::Template => false,
            MatchCategory::RemoteType => category.matches(&remote.ty.to_string(), &term.value),
        },
        Some(Err(_)) => false,
        None => {
            MatchCategory::Name.matches(remote_name, &term.value)
                || MatchCategory::Type.matches("remote", &term.value)
        }
    }
}

fn remote_type_matches_search_term(remote_type: RemoteType, term: &SearchTerm) -> bool {
    match term.category.as_deref().map(|c| c.parse::<MatchCategory>()) {
        Some(Ok(category)) => match category {
            MatchCategory::RemoteType => category.matches(&remote_type.to_string(), &term.value),
            _ => true,
        },
        Some(Err(_)) => false,
        None => true,
    }
}

// Transient type for remote resources gathering and filtering on remote properties
pub(crate) struct RemoteWithResources {
    remote_name: String,
    remote: Remote,
    resources: Vec<Resource>,
    error: Option<String>,
}

impl From<RemoteWithResources> for RemoteResources {
    fn from(val: RemoteWithResources) -> Self {
        RemoteResources {
            remote: val.remote_name,
            resources: val.resources,
            error: val.error,
        }
    }
}

#[api(
    // FIXME:: see list-like API calls in resource routers, we probably want more fine-grained
    // checks..
    access: {
        permission: &Permission::Anybody,
    },
    input: {
        properties: {
            "max-age": {
                description: "Maximum age (in seconds) of cached remote resources.",
                // TODO: What is a sensible default max-age?
                default: 30,
                optional: true,
            },
            "search": {
                description: "Search term to filter for, uses special syntax e.g. <TODO>",
                optional: true,
            },
            "resource-type": {
                type: ResourceType,
                optional: true,
            },
        }
    },
    returns: {
        description: "Array of resources, grouped by remote",
        type: Array,
        items: {
            type: RemoteResources,
        }
    },
)]
/// List all resources from remote nodes.
pub async fn get_resources(
    max_age: u64,
    resource_type: Option<ResourceType>,
    search: Option<String>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<RemoteResources>, Error> {
    let remotes_with_resources =
        get_resources_impl(max_age, search, resource_type, Some(rpcenv)).await?;
    let resources = remotes_with_resources.into_iter().map(Into::into).collect();
    Ok(resources)
}

// helper to determine if the combination of search terms requires the results
// to be remotes, so we can skip looking at resources
fn is_remotes_only(filters: &Search) -> bool {
    let mut is_required = false;
    let mut optional_matches = 0;
    let mut optional_terms = 0;
    filters.matches(|term| {
        if term.is_optional() {
            optional_terms += 1;
        }
        match term.category.as_deref().map(|c| c.parse::<MatchCategory>()) {
            Some(Ok(MatchCategory::Type)) if MatchCategory::Type.matches("remote", &term.value) => {
                if !term.is_optional() {
                    is_required = true;
                } else {
                    optional_matches += 1;
                }
            }
            _ => {}
        }
        // search is short-circuited, so to iterate over all, return true on required and false on optional
        !term.is_optional()
    });

    is_required || (optional_matches > 0 && optional_matches == optional_terms)
}

// called from resource_cache where no RPCEnvironment is initialized..
pub(crate) async fn get_resources_impl(
    max_age: u64,
    search: Option<String>,
    resource_type: Option<ResourceType>,
    rpcenv: Option<&mut dyn RpcEnvironment>,
) -> Result<Vec<RemoteWithResources>, Error> {
    let user_info = CachedUserInfo::new()?;
    let mut opt_auth_id = None;
    if let Some(ref rpcenv) = rpcenv {
        let auth_id: Authid = rpcenv
            .get_auth_id()
            .ok_or_else(|| format_err!("no authid available"))?
            .parse()?;
        if !user_info.any_privs_below(&auth_id, &["resource"], PRIV_RESOURCE_AUDIT)? {
            http_bail!(FORBIDDEN, "user has no access to resources");
        }
        opt_auth_id = Some(auth_id);
    }

    let (remotes_config, _) = pdm_config::remotes::config()?;
    let mut join_handles = Vec::new();

    let filters = search.map(Search::from).unwrap_or_default();

    let remotes_only = is_remotes_only(&filters);

    for (remote_name, remote) in remotes_config {
        if let Some(ref auth_id) = opt_auth_id {
            let remote_privs = user_info.lookup_privs(auth_id, &["resource", &remote_name]);
            if remote_privs & PRIV_RESOURCE_AUDIT == 0 {
                continue;
            }
        }

        if !filters.matches(|term| remote_type_matches_search_term(remote.ty, term)) {
            continue;
        }

        if remotes_only
            && !filters
                .matches(|term| remote_matches_search_term(&remote_name, &remote, None, term))
        {
            continue;
        }
        let filter = filters.clone();
        let handle = tokio::spawn(async move {
            let (mut resources, error) = match get_resources_for_remote(&remote, max_age).await {
                Ok(resources) => (resources, None),
                Err(error) => {
                    tracing::debug!("failed to get resources from remote - {error:?}");
                    (Vec::new(), Some(error.root_cause().to_string()))
                }
            };

            if remotes_only {
                resources.clear();
            } else if resource_type.is_some() || !filter.is_empty() {
                resources.retain(|resource| {
                    if let Some(resource_type) = resource_type {
                        if resource.resource_type() != resource_type {
                            return false;
                        }
                    }

                    filter.matches(|filter| {
                        // if we get can't decide if it matches, don't filter it out
                        resource_matches_search_term(&remote_name, resource, filter).unwrap_or(true)
                    })
                });
            }

            RemoteWithResources {
                remote_name,
                remote,
                resources,
                error,
            }
        });

        join_handles.push(handle);
    }

    let mut remote_resources = Vec::new();
    for handle in join_handles {
        let remote_with_resources = handle.await?;

        if filters.is_empty() {
            remote_resources.push(remote_with_resources);
        } else if !remote_with_resources.resources.is_empty() {
            remote_resources.push(remote_with_resources);
        } else if filters.matches(|filter| {
            remote_matches_search_term(
                &remote_with_resources.remote_name,
                &remote_with_resources.remote,
                Some(remote_with_resources.error.is_none()),
                filter,
            )
        }) {
            remote_resources.push(remote_with_resources);
        }
    }

    Ok(remote_resources)
}

#[api(
    // FIXME:: see list-like API calls in resource routers..
    access: {
        permission: &Permission::Anybody,
    },
    input: {
        properties: {
            "max-age": {
                description: "Maximum age (in seconds) of cached remote resources.",
                // TODO: What is a sensible default max-age?
                default: 30,
                optional: true,
            },
        }
    },
    returns: {
        description: "Array of resources, grouped by remote",
        type: Array,
        items: {
            type: RemoteResources,
        }
    },
)]
/// Return the amount of configured/seen resources by type
pub async fn get_status(
    max_age: u64,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<ResourcesStatus, Error> {
    let remotes_with_resources = get_resources_impl(max_age, None, None, Some(rpcenv)).await?;
    let mut counts = ResourcesStatus::default();
    for remote_with_resources in remotes_with_resources {
        if let Some(err) = remote_with_resources.error {
            counts.failed_remotes += 1;
            counts.failed_remotes_list.push(FailedRemote {
                name: remote_with_resources.remote_name,
                error: err.to_string(),
                remote_type: remote_with_resources.remote.ty,
            });
        } else {
            counts.remotes += 1;
        }
        for resource in remote_with_resources.resources {
            match resource {
                Resource::PveStorage(r) => match r.status.as_str() {
                    "available" => counts.storages.available += 1,
                    _ => counts.storages.unknown += 1,
                },
                Resource::PveQemu(r) => match r.status.as_str() {
                    _ if r.template => counts.qemu.template += 1,
                    "running" => counts.qemu.running += 1,
                    "stopped" => counts.qemu.stopped += 1,
                    _ => counts.qemu.unknown += 1,
                },
                Resource::PveLxc(r) => match r.status.as_str() {
                    _ if r.template => counts.lxc.template += 1,
                    "running" => counts.lxc.running += 1,
                    "stopped" => counts.lxc.stopped += 1,
                    _ => counts.lxc.unknown += 1,
                },
                Resource::PveNode(r) => match r.status.as_str() {
                    "online" => counts.pve_nodes.online += 1,
                    "offline" => counts.pve_nodes.offline += 1,
                    _ => counts.pve_nodes.unknown += 1,
                },
                Resource::PveSdn(r) => {
                    let PveSdnResource::Zone(_) = &r;

                    match r.status() {
                        SdnStatus::Available => {
                            counts.sdn_zones.available += 1;
                        }
                        SdnStatus::Error => {
                            counts.sdn_zones.error += 1;
                        }
                        SdnStatus::Unknown => {
                            counts.sdn_zones.unknown += 1;
                        }
                    }
                }
                // FIXME better status for pbs/datastores
                Resource::PbsNode(_) => counts.pbs_nodes.online += 1,
                Resource::PbsDatastore(r) => {
                    if r.maintenance.is_none() {
                        counts.pbs_datastores.online += 1;
                    } else {
                        *counts
                            .pbs_datastores
                            .under_maintenance
                            .get_or_insert_default() += 1;
                    }
                    if r.usage > PBS_DATASTORE_HIGH_USAGE_THRESHOLD {
                        *counts.pbs_datastores.high_usage.get_or_insert_default() += 1;
                    }
                    if r.backing_device.is_some() {
                        *counts.pbs_datastores.removable.get_or_insert_default() += 1;
                    }
                    match r.backend_type.as_deref() {
                        Some("s3") => {
                            *counts.pbs_datastores.s3_backend.get_or_insert_default() += 1;
                        }
                        Some("unknown") => {
                            *counts.pbs_datastores.unknown.get_or_insert_default() += 1;
                        }
                        _ => (),
                    }
                }
            }
        }
    }

    Ok(counts)
}

#[api(
    access: { permission: &Permission::Anybody, },
    input: {
        properties: {
            "max-age": {
                description: "Maximum age (in seconds) of cached remote subscription state.",
                // long default to not query it too often
                default: 24*60*60,
                optional: true,
            },
            // FIXME: which privileges should be necessary for returning the keys?
            verbose: {
                type: bool,
                optional: true,
                default: false,
                description: "If true, includes subscription information per node (with enough privileges)",
            },
        },
    },
    returns: {
        description: "Subscription state for each remote.",
        type: Array,
        items: {
            type: RemoteSubscriptions,
        }
    },
)]
/// Returns the subscription status of the remotes
pub async fn get_subscription_status(
    max_age: u64,
    verbose: bool,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<RemoteSubscriptions>, Error> {
    let (remotes_config, _) = pdm_config::remotes::config()?;

    let mut futures = Vec::new();

    let auth_id = rpcenv.get_auth_id().unwrap().parse()?;
    let user_info = CachedUserInfo::new()?;
    let allow_all = user_info
        .check_privs(&auth_id, &["resource"], PRIV_RESOURCE_AUDIT, false)
        .is_ok();

    let check_priv = |remote_name: &str| -> bool {
        user_info
            .check_privs(
                &auth_id,
                &["resource", remote_name],
                PRIV_RESOURCE_AUDIT,
                false,
            )
            .is_err()
    };

    for (remote_name, remote) in remotes_config {
        if !allow_all && !check_priv(&remote_name) {
            continue;
        }

        let future = async move {
            let (node_status, error) =
                match get_subscription_info_for_remote(&remote, max_age).await {
                    Ok(node_status) => (Some(node_status), None),
                    Err(error) => (None, Some(error.to_string())),
                };

            let mut state = RemoteSubscriptionState::Unknown;

            if let Some(node_status) = &node_status {
                state = map_node_subscription_list_to_state(node_status);
            }

            RemoteSubscriptions {
                remote: remote_name,
                error,
                state,
                node_status: if verbose { node_status } else { None },
            }
        };

        futures.push(future);
    }

    Ok(join_all(futures).await)
}

// FIXME: make timeframe and count parameters?
#[api(
    input: {
        properties: {
            "timeframe": {
                type: RrdTimeframe,
                optional: true,
            }
        }
    },
    access: {
        permission: &Permission::Anybody,
        description: "The user needs to have at least `Resource.Audit` on one resources under `/resource`.
        Only resources for which the user has `Resource.Audit` on `/resource/{remote_name}` will be
        considered when calculating the top entities."
    },
)]
/// Returns the top X entities regarding the chosen type
async fn get_top_entities(
    timeframe: Option<RrdTimeframe>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<TopEntities, Error> {
    let user_info = CachedUserInfo::new()?;
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .ok_or_else(|| format_err!("no authid available"))?
        .parse()?;

    if !user_info.any_privs_below(&auth_id, &["resource"], PRIV_RESOURCE_AUDIT)? {
        http_bail!(FORBIDDEN, "user has no access to resources");
    }

    let (remotes_config, _) = pdm_config::remotes::config()?;
    let check_remote_privs = |remote_name: &str| {
        user_info.lookup_privs(&auth_id, &["resource", remote_name]) & PRIV_RESOURCE_AUDIT != 0
    };

    let timeframe = timeframe.unwrap_or(RrdTimeframe::Day);
    let res = top_entities::calculate_top(&remotes_config, timeframe, 10, check_remote_privs);
    Ok(res)
}

#[derive(Clone)]
struct CachedSubscriptionState {
    node_info: HashMap<String, Option<NodeSubscriptionInfo>>,
    timestamp: i64,
}

static SUBSCRIPTION_CACHE: LazyLock<RwLock<HashMap<String, CachedSubscriptionState>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Get the subscription state for a given remote.
///
/// If recent enough cached data is available, it is returned
/// instead of calling out to the remote.
async fn get_subscription_info_for_remote(
    remote: &Remote,
    max_age: u64,
) -> Result<HashMap<String, Option<NodeSubscriptionInfo>>, Error> {
    if let Some(cached_subscription) = get_cached_subscription_info(&remote.id, max_age) {
        Ok(cached_subscription.node_info)
    } else {
        let node_info = fetch_remote_subscription_info(remote).await?;
        let now = proxmox_time::epoch_i64();
        update_cached_subscription_info(&remote.id, &node_info, now);
        Ok(node_info)
    }
}

fn get_cached_subscription_info(remote: &str, max_age: u64) -> Option<CachedSubscriptionState> {
    let cache = SUBSCRIPTION_CACHE
        .read()
        .expect("subscription mutex poisoned");

    if let Some(cached_subscription) = cache.get(remote) {
        let now = proxmox_time::epoch_i64();
        let diff = now - cached_subscription.timestamp;

        if diff > max_age as i64 || diff < 0 {
            // value is too old or from the future
            None
        } else {
            Some(cached_subscription.clone())
        }
    } else {
        None
    }
}

/// Update cached subscription data.
///
/// If the cache already contains more recent data we don't insert the passed resources.
fn update_cached_subscription_info(
    remote: &str,
    node_info: &HashMap<String, Option<NodeSubscriptionInfo>>,
    now: i64,
) {
    // there is no good way to recover from this, so panicking should be fine
    let mut cache = SUBSCRIPTION_CACHE
        .write()
        .expect("subscription mutex poisoned");

    if let Some(cached_resource) = cache.get(remote) {
        // skip updating if the data is new enough
        if cached_resource.timestamp >= now {
            return;
        }
    }

    cache.insert(
        remote.into(),
        CachedSubscriptionState {
            node_info: node_info.clone(),
            timestamp: now,
        },
    );
}

/// Maps a list of node subscription infos into a single [`RemoteSubscriptionState`]
///
/// Unavailable subscription infos should be represented as `None`
fn map_node_subscription_list_to_state(
    infos: &HashMap<String, Option<NodeSubscriptionInfo>>,
) -> RemoteSubscriptionState {
    let levels: Vec<SubscriptionLevel> = infos
        .values()
        .map(|info| match info {
            Some(info) => match info.status {
                SubscriptionStatus::New | SubscriptionStatus::Active => info.level,
                _ => SubscriptionLevel::None,
            },
            None => SubscriptionLevel::Unknown,
        })
        .collect();

    let minimum = levels
        .iter()
        .min()
        .copied()
        .unwrap_or(SubscriptionLevel::Unknown);
    let mixed = levels.iter().any(|level| *level != minimum);

    match (minimum, mixed) {
        (SubscriptionLevel::None, _) => RemoteSubscriptionState::None,
        (SubscriptionLevel::Unknown, false) => RemoteSubscriptionState::Unknown,
        // treat unknown + active as active
        (SubscriptionLevel::Unknown, true) => RemoteSubscriptionState::Active,
        (_, true) => RemoteSubscriptionState::Mixed,
        (_, false) => RemoteSubscriptionState::Active,
    }
}

/// Fetch remote resources and map to pdm-native data types.
async fn fetch_remote_subscription_info(
    remote: &Remote,
) -> Result<HashMap<String, Option<NodeSubscriptionInfo>>, Error> {
    let mut list = HashMap::new();
    match remote.ty {
        RemoteType::Pve => {
            let client = connection::make_pve_client(remote)?;

            let nodes = client.list_nodes().await?;
            let mut futures = Vec::with_capacity(nodes.len());
            for node in nodes.iter() {
                let future = client.get_subscription(&node.node).map(|res| res.ok());
                futures.push(async move { (node.node.clone(), future.await) });
            }

            for (node_name, remote_info) in join_all(futures).await {
                list.insert(
                    node_name,
                    remote_info.map(|info| {
                        let status = serde_json::to_value(info.status)
                            .map(|status| serde_json::from_value(status).unwrap_or_default())
                            .unwrap_or_default();
                        NodeSubscriptionInfo {
                            status,
                            sockets: info.sockets,
                            key: info.key,
                            level: info
                                .level
                                .and_then(|level| level.parse().ok())
                                .unwrap_or_default(),
                        }
                    }),
                );
            }
        }
        RemoteType::Pbs => {
            let client = connection::make_pbs_client(remote)?;

            let info = client.get_subscription().await.ok().map(|info| {
                let level = SubscriptionLevel::from_key(info.key.as_deref());
                NodeSubscriptionInfo {
                    status: info.status,
                    sockets: None,
                    key: info.key,
                    level,
                }
            });

            list.insert("localhost".to_string(), info);
        }
    };

    Ok(list)
}

#[derive(Clone)]
pub struct CachedResources {
    pub resources: Vec<Resource>,
    pub timestamp: i64,
}

static CACHE: LazyLock<RwLock<HashMap<String, CachedResources>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Get resources for a given remote.
///
/// If recent enough cached data is available, it is returned
/// instead of calling out to the remote.
async fn get_resources_for_remote(remote: &Remote, max_age: u64) -> Result<Vec<Resource>, Error> {
    let remote_name = remote.id.to_owned();
    if let Some(cached_resource) = get_cached_resources(&remote_name, max_age) {
        Ok(cached_resource.resources)
    } else {
        let resources = fetch_remote_resource(remote).await?;
        let now = proxmox_time::epoch_i64();
        update_cached_resources(&remote_name, &resources, now);
        Ok(resources)
    }
}

/// Read cached resource data from the cache
pub fn get_cached_resources(remote: &str, max_age: u64) -> Option<CachedResources> {
    // there is no good way to recover from this, so panicking should be fine
    let cache = CACHE.read().expect("mutex poisoned");

    if let Some(cached_resource) = cache.get(remote) {
        let now = proxmox_time::epoch_i64();
        let diff = now - cached_resource.timestamp;

        if diff > max_age as i64 || diff < 0 {
            // value is too old or from the future
            None
        } else {
            Some(cached_resource.clone())
        }
    } else {
        None
    }
}

/// Update cached resource data.
///
/// If the cache already contains more recent data we don't insert the passed resources.
fn update_cached_resources(remote: &str, resources: &[Resource], now: i64) {
    // there is no good way to recover from this, so panicking should be fine
    let mut cache = CACHE.write().expect("mutex poisoned");

    if let Some(cached_resource) = cache.get(remote) {
        // skip updating if existing data is newer
        if cached_resource.timestamp >= now {
            return;
        }
    }

    cache.insert(
        remote.into(),
        CachedResources {
            timestamp: now,
            resources: resources.into(),
        },
    );
}

/// Fetch remote resources and map to pdm-native data types.
async fn fetch_remote_resource(remote: &Remote) -> Result<Vec<Resource>, Error> {
    let mut resources = Vec::new();
    let remote_name = remote.id.to_owned();

    match remote.ty {
        RemoteType::Pve => {
            let client = connection::make_pve_client(remote)?;

            let cluster_resources = client.cluster_resources(None).await?;

            for resource in cluster_resources {
                if let Some(r) = map_pve_resource(&remote_name, resource) {
                    resources.push(r);
                }
            }
        }
        RemoteType::Pbs => {
            let client = connection::make_pbs_client(remote)?;

            let status = client.node_status().await?;
            resources.push(map_pbs_node_status(&remote_name, status));

            let datastores = client.list_datastores().await?;
            let datastore_map: HashMap<String, pbs_api_types::DataStoreConfig> = datastores
                .into_iter()
                .map(|store| (store.name.clone(), store))
                .collect();

            for datastore_usage in client.datastore_usage().await? {
                resources.push(map_pbs_datastore_status(
                    &remote_name,
                    datastore_usage,
                    &datastore_map,
                ));
            }
        }
    }

    Ok(resources)
}

pub(super) fn map_pve_node(remote: &str, resource: ClusterResource) -> Option<PveNodeResource> {
    match resource.ty {
        ClusterResourceType::Node => Some(PveNodeResource {
            cgroup_mode: resource.cgroup_mode.unwrap_or_default(),
            cpu: resource.cpu.unwrap_or_default(),
            maxcpu: resource.maxcpu.unwrap_or_default(),
            id: format!(
                "remote/{remote}/node/{}",
                &resource.node.clone().unwrap_or_default()
            ),
            mem: resource.mem.unwrap_or_default(),
            maxmem: resource.maxmem.unwrap_or_default() as u64,
            node: resource.node.unwrap_or_default(),
            uptime: resource.uptime.unwrap_or_default() as u64,
            status: resource.status.unwrap_or_default(),
            level: resource.level.unwrap_or_default(),
        }),
        _ => None,
    }
}

pub(super) fn map_pve_lxc(remote: &str, resource: ClusterResource) -> Option<PveLxcResource> {
    match resource.ty {
        ClusterResourceType::Lxc => Some(PveLxcResource {
            cpu: resource.cpu.unwrap_or_default(),
            maxcpu: resource.maxcpu.unwrap_or_default(),
            disk: resource.disk.unwrap_or_default(),
            maxdisk: resource.maxdisk.unwrap_or_default(),
            id: format!(
                "remote/{remote}/guest/{}",
                &resource.vmid.unwrap_or_default()
            ),
            mem: resource.mem.unwrap_or_default(),
            maxmem: resource.maxmem.unwrap_or_default() as u64,
            name: resource.name.unwrap_or_default(),
            node: resource.node.unwrap_or_default(),
            pool: resource.pool.unwrap_or_default(),
            status: resource.status.unwrap_or_default(),
            tags: resource
                .tags
                .map(|tags| {
                    tags.as_str()
                        .split(&[';', ',', ' '])
                        .filter_map(|s| (!s.is_empty()).then_some(s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            template: resource.template.unwrap_or_default(),
            uptime: resource.uptime.unwrap_or_default() as u64,
            vmid: resource.vmid.unwrap_or_default(),
        }),
        _ => None,
    }
}

pub(super) fn map_pve_qemu(remote: &str, resource: ClusterResource) -> Option<PveQemuResource> {
    match resource.ty {
        ClusterResourceType::Qemu => Some(PveQemuResource {
            cpu: resource.cpu.unwrap_or_default(),
            maxcpu: resource.maxcpu.unwrap_or_default(),
            disk: resource.disk.unwrap_or_default(),
            maxdisk: resource.maxdisk.unwrap_or_default(),
            id: format!(
                "remote/{remote}/guest/{}",
                &resource.vmid.unwrap_or_default()
            ),
            mem: resource.mem.unwrap_or_default(),
            maxmem: resource.maxmem.unwrap_or_default() as u64,
            name: resource.name.unwrap_or_default(),
            node: resource.node.unwrap_or_default(),
            pool: resource.pool.unwrap_or_default(),
            status: resource.status.unwrap_or_default(),
            tags: resource
                .tags
                .map(|tags| {
                    tags.as_str()
                        .split(&[';', ',', ' '])
                        .filter_map(|s| (!s.is_empty()).then_some(s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            template: resource.template.unwrap_or_default(),
            uptime: resource.uptime.unwrap_or_default() as u64,
            vmid: resource.vmid.unwrap_or_default(),
        }),
        _ => None,
    }
}

pub(super) fn map_pve_storage(
    remote: &str,
    resource: ClusterResource,
) -> Option<PveStorageResource> {
    match resource.ty {
        ClusterResourceType::Storage => Some(PveStorageResource {
            disk: resource.disk.unwrap_or_default(),
            maxdisk: resource.maxdisk.unwrap_or_default(),
            id: format!("remote/{remote}/{}", &resource.id),
            storage: resource.storage.unwrap_or_default(),
            node: resource.node.unwrap_or_default(),
            status: resource.status.unwrap_or_default(),
        }),
        _ => None,
    }
}

pub(super) fn map_pve_sdn(remote: &str, resource: ClusterResource) -> Option<PveSdnResource> {
    match resource.ty {
        ClusterResourceType::Sdn => {
            let node = resource.node.unwrap_or_default();

            Some(PveSdnResource::Zone(SdnZoneResource {
                id: format!("remote/{remote}/sdn/{}", &resource.id),
                name: resource.sdn.unwrap_or_default(),
                node,
                status: SdnStatus::from_str(resource.status.unwrap_or_default().as_str())
                    .unwrap_or_default(),
            }))
        }
        _ => None,
    }
}

fn map_pve_resource(remote: &str, resource: ClusterResource) -> Option<Resource> {
    match resource.ty {
        ClusterResourceType::Node => map_pve_node(remote, resource).map(Resource::PveNode),
        ClusterResourceType::Lxc => map_pve_lxc(remote, resource).map(Resource::PveLxc),
        ClusterResourceType::Qemu => map_pve_qemu(remote, resource).map(Resource::PveQemu),
        ClusterResourceType::Storage => map_pve_storage(remote, resource).map(Resource::PveStorage),
        ClusterResourceType::Sdn => map_pve_sdn(remote, resource).map(Resource::PveSdn),
        _ => None,
    }
}

fn map_pbs_node_status(remote: &str, status: NodeStatus) -> Resource {
    Resource::PbsNode(PbsNodeResource {
        cpu: status.cpu,
        maxcpu: status.cpuinfo.cpus as f64,
        // TODO: Right now there is no API to get the actual node name, as it seems
        id: format!("remote/{remote}/node/localhost"),
        name: "localhost".into(),
        mem: status.memory.used,
        maxmem: status.memory.total,
        uptime: status.uptime,
    })
}

fn map_pbs_datastore_status(
    remote: &str,
    status: DataStoreStatusListItem,
    datastore_map: &HashMap<String, pbs_api_types::DataStoreConfig>,
) -> Resource {
    let maxdisk = status.total.unwrap_or_default();
    let disk = status.used.unwrap_or_default();

    let usage = if maxdisk > 0 {
        disk as f64 / maxdisk as f64
    } else {
        0.0
    };

    if let Some(store_config) = datastore_map.get(&status.store) {
        let mut backend_type = None;
        if let Some(store_backend) = &store_config.backend {
            match store_backend.parse::<DatastoreBackendConfig>() {
                Ok(backend_config) => {
                    if let Some(DatastoreBackendType::S3) = backend_config.ty {
                        backend_type = Some(DatastoreBackendType::S3.to_string())
                    }
                }
                Err(_) => backend_type = Some("unknown".to_string()),
            }
        }
        Resource::PbsDatastore(PbsDatastoreResource {
            id: format!("remote/{remote}/datastore/{}", status.store),
            name: status.store,
            maxdisk,
            disk,
            usage,
            maintenance: store_config.maintenance_mode.clone(),
            backing_device: store_config.backing_device.clone(),
            backend_type,
        })
    } else {
        Resource::PbsDatastore(PbsDatastoreResource {
            id: format!("remote/{remote}/datastore/{}", status.store),
            name: status.store,
            maxdisk,
            disk,
            usage,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::api::resources::is_remotes_only;
    use pdm_search::{Search, SearchTerm};

    #[test]
    fn is_remote_only() {
        let remote_term = SearchTerm::new("foo").category(Some("remote"));
        let remote_term_optional = remote_term.clone().optional(true);

        let other_term = SearchTerm::new("foo");
        let other_term_optional = other_term.clone().optional(true);

        let type_remote_term = SearchTerm::new("remote").category(Some("type"));
        let type_remote_term_optional = type_remote_term.clone().optional(true);

        let type_other_term = SearchTerm::new("foo").category(Some("type"));
        let type_other_term_optional = type_other_term.clone().optional(true);

        let cases = vec![
            (vec![other_term.clone()], false),
            (vec![other_term_optional.clone()], false),
            (vec![remote_term.clone()], false),
            (vec![remote_term_optional.clone()], false),
            (vec![type_other_term.clone()], false),
            (vec![type_other_term_optional.clone()], false),
            (
                vec![SearchTerm::new("re").optional(true).category(Some("type"))],
                true,
            ),
            (vec![type_remote_term.clone()], true),
            (vec![type_remote_term_optional.clone()], true),
            (
                vec![
                    type_remote_term_optional.clone(),
                    other_term_optional.clone(),
                ],
                false,
            ),
            (
                vec![
                    type_other_term_optional.clone(),
                    other_term_optional.clone(),
                ],
                false,
            ),
            (
                vec![
                    type_remote_term.clone(),
                    type_other_term_optional.clone(),
                    other_term_optional.clone(),
                ],
                true,
            ),
            (vec![other_term.clone(), type_remote_term.clone()], true),
        ];

        for (count, (case, expected)) in cases.into_iter().enumerate() {
            let search = Search::from_iter(case.into_iter());
            assert_eq!(is_remotes_only(&search), expected, "case: {count}");
        }
    }
}
