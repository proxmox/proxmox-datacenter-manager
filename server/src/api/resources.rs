use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use anyhow::{Context, Error, bail};
use futures::FutureExt;
use futures::future::join_all;

use pbs_api_types::{
    DataStoreStatusListItem, DatastoreBackendConfig, DatastoreBackendType, NodeStatus,
};
use pdm_api_types::remotes::{Remote, RemoteType};
use pdm_api_types::resource::{
    FailedRemote, NetworkFabricResource, NetworkZoneResource, PBS_DATASTORE_HIGH_USAGE_THRESHOLD,
    PbsDatastoreResource, PbsNodeResource, PveLxcResource, PveNetworkResource, PveNodeResource,
    PveQemuResource, PveStorageResource, RemoteInfo, RemoteResources, RemoteStatus, Resource,
    ResourceType, ResourcesStatus, SdnStatus, TopEntities,
};
use pdm_api_types::subscription::{
    NodeSubscriptionInfo, RemoteSubscriptionState, RemoteSubscriptions, SubscriptionLevel,
};
use pdm_api_types::{Authid, CachedLocationInfo, PRIV_RESOURCE_AUDIT, VIEW_ID_SCHEMA};
use pdm_search::{Search, SearchTerm};
use proxmox_access_control::CachedUserInfo;
use proxmox_router::{
    Permission, Router, RpcEnvironment, SubdirMap, http_bail, list_subdirs_api_method,
};
use proxmox_rrd_api_types::RrdTimeframe;
use proxmox_schema::{api, parse_boolean};
use proxmox_sortable_macro::sortable;
use proxmox_subscription::SubscriptionStatus;
use pve_api_types::{ClusterResource, ClusterResourceNetworkType, ClusterResourceType};
use serde::{Deserialize, Serialize};

use crate::metric_collection::top_entities;
use crate::{api_cache, connection, views};

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("list", &Router::new().get(&API_METHOD_GET_RESOURCES)),
    (
        "location-info",
        &Router::new().get(&API_METHOD_GET_LOCATION_INFO)
    ),
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
    NetworkType,
    Name,
    Id,
    Status,
    Template,
    Remote,
    RemoteType,
    Property,
    View,
}

impl std::str::FromStr for MatchCategory {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let category = match s {
            "type" => MatchCategory::Type,
            "network-type" => MatchCategory::NetworkType,
            "name" => MatchCategory::Name,
            "id" => MatchCategory::Id,
            "status" => MatchCategory::Status,
            "template" => MatchCategory::Template,
            "remote" => MatchCategory::Remote,
            "remote-type" => MatchCategory::RemoteType,
            "property" => MatchCategory::Property,
            "view" => MatchCategory::View,
            _ => bail!("invalid category"),
        };
        Ok(category)
    }
}

impl MatchCategory {
    fn matches(&self, value: &str, search_term: &str) -> bool {
        match self {
            MatchCategory::Type | MatchCategory::Status | MatchCategory::NetworkType => value
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
            MatchCategory::View => true,
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
            MatchCategory::NetworkType => match resource {
                Resource::PveNetwork(network_resource) => {
                    category.matches(network_resource.network_type().as_str(), &term.value)
                }
                _ => false,
            },
            MatchCategory::View => return None,
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
            MatchCategory::NetworkType => false,
            MatchCategory::View => true,
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

fn check_remote_priv(user_info: &CachedUserInfo, auth_id: &Authid, remote: &str) -> bool {
    user_info
        .check_privs(auth_id, &["resource", remote], PRIV_RESOURCE_AUDIT, false)
        .is_ok()
}

/// When returning true, all remotes are allowed and no per-remote permission check should be
/// necessary
fn check_all_remotes_allowed(
    user_info: &CachedUserInfo,
    auth_id: &Authid,
    view: Option<&str>,
) -> Result<bool, Error> {
    Ok(if let Some(view) = view {
        user_info.check_privs(auth_id, &["view", view], PRIV_RESOURCE_AUDIT, false)?;
        false
    } else {
        user_info
            .check_privs(auth_id, &["resource"], PRIV_RESOURCE_AUDIT, false)
            .is_ok()
    })
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
            view: {
                schema: VIEW_ID_SCHEMA,
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
    view: Option<String>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<RemoteResources>, Error> {
    let remotes_with_resources = get_resources_impl(
        max_age,
        search,
        resource_type,
        view.as_deref(),
        Some(rpcenv),
    )
    .await?;
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
    view: Option<&str>,
    rpcenv: Option<&mut dyn RpcEnvironment>,
) -> Result<Vec<RemoteWithResources>, Error> {
    let user_info = CachedUserInfo::new()?;
    let mut opt_auth_id = None;
    if let Some(ref rpcenv) = rpcenv {
        let auth_id: Authid = rpcenv
            .get_auth_id()
            .context("no authid available")?
            .parse()?;

        // NOTE: Assumption is that the regular permission check is completely replaced by a check
        // on the view ACL object *if* a view parameter is passed.
        if let Some(view) = &view {
            user_info.check_privs(&auth_id, &["view", view], PRIV_RESOURCE_AUDIT, false)?;
        } else if !user_info.any_privs_below(&auth_id, &["resource"], PRIV_RESOURCE_AUDIT)? {
            http_bail!(FORBIDDEN, "user has no access to resources");
        }

        opt_auth_id = Some(auth_id);
    }

    let (remotes_config, _) = pdm_config::remotes::config()?;
    let mut join_handles = Vec::new();

    let filters = search.map(Search::from).unwrap_or_default();

    let view = views::get_optional_view(view)?;

    let mut view_filter_from_search = None;
    filters.matches(|term| {
        if let Some("view") = term.category.as_deref() {
            view_filter_from_search = Some(term.value.to_string());
        }
        true
    });

    let view = view.or(views::get_optional_view(
        view_filter_from_search.as_deref(),
    )?);

    let remotes_only = is_remotes_only(&filters);

    for (remote_name, remote) in remotes_config {
        if let Some(view) = &view {
            if view.can_skip_remote(&remote_name) {
                continue;
            }
        } else if let Some(ref auth_id) = opt_auth_id {
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

        if filters.is_empty()
            || !remote_with_resources.resources.is_empty()
            || filters.matches(|filter| {
                remote_matches_search_term(
                    &remote_with_resources.remote_name,
                    &remote_with_resources.remote,
                    Some(remote_with_resources.error.is_none()),
                    filter,
                )
            })
        {
            remote_resources.push(remote_with_resources);
        }
    }

    if let Some(view) = &view {
        remote_resources.retain_mut(|r| {
            r.resources
                .retain(|resource| view.resource_matches(&r.remote_name, resource));

            let has_any_matched_resources = !r.resources.is_empty();
            has_any_matched_resources
                || (r.error.is_some() && view.is_remote_explicitly_included(&r.remote_name))
        });
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
            view: {
                schema: VIEW_ID_SCHEMA,
                optional: true,
            },
        }
    },
    returns: { type: RemoteResources },
)]
/// Return the amount of configured/seen resources by type
pub async fn get_status(
    max_age: u64,
    view: Option<String>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<ResourcesStatus, Error> {
    let remotes_with_resources =
        get_resources_impl(max_age, None, None, view.as_deref(), Some(rpcenv)).await?;
    let mut counts = ResourcesStatus::default();
    let mut pve_cpu_allocated = 0.0;
    for remote_with_resources in remotes_with_resources {
        if let Some(err) = &remote_with_resources.error {
            counts.failed_remotes += 1;
            counts.failed_remotes_list.push(FailedRemote {
                name: remote_with_resources.remote_name.clone(),
                error: err.to_string(),
                remote_type: remote_with_resources.remote.ty,
            });
        } else {
            counts.remotes += 1;
        }

        let mut remote_status = if remote_with_resources.error.is_some() {
            RemoteStatus::Error
        } else {
            RemoteStatus::Good
        };
        let mut remote_messages = match remote_with_resources.error {
            Some(error) => vec![error],
            None => Vec::new(),
        };

        let mut seen_storages = HashSet::new();
        for resource in remote_with_resources.resources {
            match resource {
                Resource::PveStorage(r) => {
                    match r.status.as_str() {
                        "available" => counts.storages.available += 1,
                        _ => counts.storages.unknown += 1,
                    }
                    if !r.shared || !seen_storages.contains(&r.storage) {
                        counts.pve_storage_stats.total += r.maxdisk;
                        counts.pve_storage_stats.used += r.disk;
                        counts.pve_storage_stats.avail += r.maxdisk - r.disk;
                        seen_storages.insert(r.storage);
                    }
                }
                Resource::PveQemu(r) => match r.status.as_str() {
                    _ if r.template => counts.qemu.template += 1,
                    "running" => {
                        counts.qemu.running += 1;
                        pve_cpu_allocated += r.maxcpu;
                    }
                    "stopped" => counts.qemu.stopped += 1,
                    _ => counts.qemu.unknown += 1,
                },
                Resource::PveLxc(r) => match r.status.as_str() {
                    _ if r.template => counts.lxc.template += 1,
                    "running" => {
                        counts.lxc.running += 1;
                        pve_cpu_allocated += r.maxcpu;
                    }
                    "stopped" => counts.lxc.stopped += 1,
                    _ => counts.lxc.unknown += 1,
                },
                Resource::PveNode(r) => {
                    match r.status.as_str() {
                        "online" => counts.pve_nodes.online += 1,
                        "offline" => {
                            if remote_status == RemoteStatus::Good {
                                remote_status = RemoteStatus::Warning;
                            }
                            remote_messages.push(format!("Node '{}' is offline", r.node));
                            counts.pve_nodes.offline += 1
                        }
                        _ => counts.pve_nodes.unknown += 1,
                    }
                    counts.pve_cpu_stats.used += r.cpu * r.maxcpu;
                    counts.pve_cpu_stats.max += r.maxcpu;

                    counts.pve_memory_stats.total += r.maxmem;
                    counts.pve_memory_stats.used += r.mem;
                    counts.pve_memory_stats.avail += r.maxmem - r.mem;
                }
                Resource::PveNetwork(r) => {
                    if let PveNetworkResource::Zone(zone) = r {
                        match zone.status() {
                            SdnStatus::Available => {
                                counts.sdn_zones.available += 1;
                            }
                            SdnStatus::Error => {
                                if remote_status == RemoteStatus::Good {
                                    remote_status = RemoteStatus::Warning;
                                }
                                remote_messages
                                    .push(format!("SDN zone '{}' has an error", zone.network));
                                counts.sdn_zones.error += 1;
                            }
                            SdnStatus::Pending => {
                                counts.sdn_zones.pending += 1;
                            }
                            SdnStatus::Unknown => {
                                counts.sdn_zones.unknown += 1;
                            }
                        }
                    }
                }
                // FIXME better status for pbs/datastores
                Resource::PbsNode(r) => {
                    counts.pbs_nodes.online += 1;

                    counts.pbs_cpu_stats.used += r.cpu * r.maxcpu;
                    counts.pbs_cpu_stats.max += r.maxcpu;

                    counts.pbs_memory_stats.total += r.maxmem;
                    counts.pbs_memory_stats.used += r.mem;
                    counts.pbs_memory_stats.avail += r.maxmem - r.mem;
                }
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

                    counts.pbs_storage_stats.total += r.maxdisk;
                    counts.pbs_storage_stats.used += r.disk;
                    counts.pbs_storage_stats.avail += r.maxdisk - r.disk;
                }
            }
        }

        counts.remote_list.push(RemoteInfo {
            name: remote_with_resources.remote_name,
            ty: remote_with_resources.remote.ty,
            status: remote_status,
            messages: remote_messages,
        });
    }

    counts.pve_cpu_stats.allocated = Some(pve_cpu_allocated);

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
            view: {
                schema: VIEW_ID_SCHEMA,
                optional: true,
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
    view: Option<String>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<RemoteSubscriptions>, Error> {
    let (remotes_config, _) = pdm_config::remotes::config()?;

    let mut futures = Vec::new();

    let auth_id = rpcenv
        .get_auth_id()
        .context("no authid available")?
        .parse()?;
    let user_info = CachedUserInfo::new()?;

    let allow_all = check_all_remotes_allowed(&user_info, &auth_id, view.as_deref())?;

    let view = views::get_optional_view(view.as_deref())?;

    for (remote_name, remote) in remotes_config {
        if let Some(view) = &view {
            if view.can_skip_remote(&remote_name) {
                continue;
            }
        } else if !allow_all && !check_remote_priv(&user_info, &auth_id, &remote_name) {
            continue;
        }

        let view = view.clone();

        let future = async move {
            let (node_status, error) =
                match get_subscription_info_for_remote(&remote, max_age).await {
                    Ok(mut node_status) => {
                        node_status.retain(|node, _| {
                            if let Some(view) = &view {
                                view.is_node_included(&remote.id, node)
                            } else {
                                true
                            }
                        });
                        (Some(node_status), None)
                    }
                    Err(error) => (None, Some(error.to_string())),
                };

            if let Some(view) = view {
                if error.is_some() && !view.is_remote_explicitly_included(&remote.id) {
                    // Don't leak the existence of failed remotes unless they were explicitly
                    // pulled in by a `include remote:<id>` rule.
                    return None;
                }
            }

            let state = if let Some(node_status) = &node_status {
                if node_status.is_empty() {
                    return None;
                }

                map_node_subscription_list_to_state(node_status)
            } else {
                RemoteSubscriptionState::Unknown
            };

            Some(RemoteSubscriptions {
                remote: remote_name,
                error,
                state,
                node_status: if verbose { node_status } else { None },
            })
        };

        futures.push(future);
    }

    let status = join_all(futures).await.into_iter().flatten().collect();

    Ok(status)
}

// FIXME: make timeframe and count parameters?
#[api(
    input: {
        properties: {
            "timeframe": {
                type: RrdTimeframe,
                optional: true,
            },
            view: {
                schema: VIEW_ID_SCHEMA,
                optional: true,
            },
        }
    },
    access: {
        permission: &Permission::Anybody,
        description: "The user needs to have at least `Resource.Audit` on one resources under `/resource`.
        Only resources for which the user has `Resource.Audit` on `/resource/{remote_name}` will be
        considered when calculating the top entities."
    },
    returns: { type: TopEntities }
)]
/// Returns the top X entities regarding the chosen type
fn get_top_entities(
    timeframe: Option<RrdTimeframe>,
    view: Option<String>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<TopEntities, Error> {
    let user_info = CachedUserInfo::new()?;
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .context("no authid available")?
        .parse()?;

    if let Some(view) = &view {
        user_info.check_privs(&auth_id, &["view", view], PRIV_RESOURCE_AUDIT, false)?;
    } else if !user_info.any_privs_below(&auth_id, &["resource"], PRIV_RESOURCE_AUDIT)? {
        http_bail!(FORBIDDEN, "user has no access to resources");
    }

    let view = views::get_optional_view(view.as_deref())?;

    let (remotes_config, _) = pdm_config::remotes::config()?;

    let check_remote_privs = |remote_name: &str| {
        if let Some(view) = &view {
            // if `include-remote` or `exclude-remote` are used we can limit the
            // number of remotes to check.
            !view.can_skip_remote(remote_name)
        } else {
            user_info.lookup_privs(&auth_id, &["resource", remote_name]) & PRIV_RESOURCE_AUDIT != 0
        }
    };

    let is_resource_included = |remote: &str, resource: &Resource| {
        if let Some(view) = &view {
            view.resource_matches(remote, resource)
        } else {
            true
        }
    };

    let timeframe = timeframe.unwrap_or(RrdTimeframe::Day);
    let res = top_entities::calculate_top(
        &remotes_config,
        timeframe,
        10,
        check_remote_privs,
        is_resource_included,
    );

    Ok(res)
}

#[derive(Clone, Serialize, Deserialize)]
struct CachedSubscriptionState {
    node_info: HashMap<String, Option<NodeSubscriptionInfo>>,
}

/// Get the subscription state for a given remote.
///
/// If recent enough cached data is available, it is returned
/// instead of calling out to the remote.
pub async fn get_subscription_info_for_remote(
    remote: &Remote,
    max_age: u64,
) -> Result<HashMap<String, Option<NodeSubscriptionInfo>>, Error> {
    if let Some(cached_subscription) = get_cached_subscription_info(&remote.id, max_age).await? {
        Ok(cached_subscription.node_info)
    } else {
        let node_info = fetch_remote_subscription_info(remote).await?;
        let now = proxmox_time::epoch_i64();

        if let Some(existing_state) =
            update_cached_subscription_info(&remote.id, node_info.clone(), now).await?
        {
            // Somebody else updated the cache while we performed the API request,
            // return the more recent data instead of the data we just fetched.
            return Ok(existing_state.node_info);
        }
        Ok(node_info)
    }
}

const SUBSCRIPTION_STATE_CACHE_KEY: &str = "subscription-state";

async fn get_cached_subscription_info(
    remote: &str,
    max_age: u64,
) -> Result<Option<CachedSubscriptionState>, Error> {
    let cache = api_cache::read_remote(remote).await?;
    let subscription_state = cache
        .get_with_max_age(SUBSCRIPTION_STATE_CACHE_KEY, max_age as i64)
        .await
        .inspect_err(|err| log::error!("could not read subscription-state from API cache: {err}"))
        .ok()
        .flatten();

    Ok(subscription_state)
}

/// Drop the cached subscription state for a remote, forcing the next read to refetch.
pub async fn invalidate_subscription_info_for_remote(remote_id: &str) {
    let cache = match api_cache::write_remote(remote_id).await {
        Ok(cache) => cache,
        Err(err) => {
            log::error!("could not open API cache for {remote_id}: {err}");
            return;
        }
    };
    if let Err(err) = cache.remove(SUBSCRIPTION_STATE_CACHE_KEY).await {
        log::error!("could not invalidate subscription-state cache for {remote_id}: {err}");
    }
}

/// Update cached subscription data.
///
/// If the cache already contains more recent data, this function returns the already
/// stored state as `Ok(Some(state))`. If the data that was passed in replaced the cache
/// entry, `Ok(None)` is returned.
async fn update_cached_subscription_info(
    remote: &str,
    node_info: HashMap<String, Option<NodeSubscriptionInfo>>,
    now: i64,
) -> Result<Option<CachedSubscriptionState>, Error> {
    let cache = api_cache::write_remote(remote).await?;

    Ok(cache
        .set_if_newer_with_timestamp(
            SUBSCRIPTION_STATE_CACHE_KEY,
            CachedSubscriptionState { node_info },
            now,
        )
        .await?)
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
                let sub_fut = client.get_subscription(&node.node).map(|res| res.ok());
                // PVE's subscription endpoint only returns `sockets` once a key is registered, so
                // auto-assign needs a separate hardware-socket source for un-subscribed nodes.
                let status_fut = client.node_status(&node.node).map(|res| res.ok());
                let node_name = node.node.clone();
                futures.push(async move {
                    let (sub, status) = futures::future::join(sub_fut, status_fut).await;
                    (node_name, sub, status)
                });
            }

            for (node_name, remote_info, node_status) in join_all(futures).await {
                let hw_sockets = node_status.map(|s| s.cpuinfo.sockets);
                list.insert(
                    node_name,
                    remote_info.map(|info| {
                        let status = serde_json::to_value(info.status)
                            .map(|status| serde_json::from_value(status).unwrap_or_default())
                            .unwrap_or_default();
                        NodeSubscriptionInfo {
                            status,
                            sockets: info.sockets.or(hw_sockets),
                            key: info.key,
                            serverid: info.serverid,
                            level: info
                                .level
                                .and_then(|level| level.parse().ok())
                                .unwrap_or_default(),
                            check_time: info.checktime,
                            next_due_date: info.nextduedate,
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
                    serverid: info.serverid,
                    check_time: info.checktime,
                    next_due_date: info.nextduedate,
                }
            });

            list.insert("localhost".to_string(), info);
        }
    };

    Ok(list)
}

#[derive(Clone, Serialize, Deserialize)]
/// Per-remote entry in the resource cache.
pub struct CachedResources {
    /// Resources for this remote.
    pub resources: Vec<Resource>,
    /// Unix timestamp at which the list of resources was polled.
    pub timestamp: i64,
}

/// Get resources for a given remote.
///
/// If recent enough cached data is available, it is returned
/// instead of calling out to the remote.
async fn get_resources_for_remote(remote: &Remote, max_age: u64) -> Result<Vec<Resource>, Error> {
    if let Some(cached_resources) = get_cached_resources(&remote.id, max_age).await? {
        Ok(cached_resources.resources)
    } else {
        let resources = fetch_remote_resource(remote).await?;
        let now = proxmox_time::epoch_i64();

        if let Some(existing_state) =
            update_cached_resources(&remote.id, resources.clone(), now).await?
        {
            // Somebody else updated the cache while we performed the API request,
            // return the more recent data instead of the data we just fetched.
            return Ok(existing_state.resources);
        }
        Ok(resources)
    }
}

const REMOTE_RESOURCES_CACHE_KEY: &str = "resources";

/// Read cached resource data from the cache
async fn get_cached_resources(
    remote: &str,
    max_age: u64,
) -> Result<Option<CachedResources>, Error> {
    let cache = api_cache::read_remote(remote).await?;
    let resources = cache
        .get_with_max_age(REMOTE_RESOURCES_CACHE_KEY, max_age as i64)
        .await
        .inspect_err(|err| {
            log::error!(
                "could not read remote resources for remote '{remote}' from API cache: {err}"
            )
        })
        .ok()
        .flatten();

    Ok(resources)
}

/// Read cached resource data from the cache (blocking).
pub fn get_cached_resources_blocking(
    remote: &str,
    max_age: u64,
) -> Result<Option<CachedResources>, Error> {
    let cache = api_cache::read_remote_blocking(remote)?;
    let resources = cache
        .get_with_max_age(REMOTE_RESOURCES_CACHE_KEY, max_age as i64)
        .inspect_err(|err| {
            log::error!(
                "could not read remote resources for remote '{remote}' from API cache: {err}"
            )
        })
        .ok()
        .flatten();

    Ok(resources)
}

/// Update cached resource data.
///
/// If the cache already contains more recent data, this function returns the already
/// stored state as `Ok(Some(state))`. If the data that was passed in replaced the cache
/// entry, `Ok(None)` is returned.
async fn update_cached_resources(
    remote: &str,
    resources: Vec<Resource>,
    now: i64,
) -> Result<Option<CachedResources>, Error> {
    let cache = api_cache::write_remote(remote).await?;

    Ok(cache
        .set_if_newer_with_timestamp(
            REMOTE_RESOURCES_CACHE_KEY,
            CachedResources {
                resources,
                timestamp: now,
            },
            now,
        )
        .await?)
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
            shared: resource.shared.unwrap_or_default(),
        }),
        _ => None,
    }
}

pub(super) fn map_pve_sdn(remote: &str, resource: ClusterResource) -> Option<PveNetworkResource> {
    match resource.ty {
        ClusterResourceType::Sdn => {
            let node = resource.node.unwrap_or_default();

            Some(PveNetworkResource::Zone(NetworkZoneResource {
                id: format!("remote/{remote}/sdn/{}", &resource.id),
                network: resource.sdn.unwrap_or_default(),
                node,
                // is empty in this format
                zone_type: resource.zone_type.unwrap_or_default(),
                status: SdnStatus::from_str(resource.status.unwrap_or_default().as_str())
                    .unwrap_or_default(),
                legacy: true,
            }))
        }
        _ => None,
    }
}

pub(super) fn map_pve_network(
    remote: &str,
    resource: ClusterResource,
) -> Option<PveNetworkResource> {
    match resource.ty {
        ClusterResourceType::Network => {
            let network_type = resource.network_type?;

            let id = format!("remote/{remote}/{}", &resource.id);
            let node = resource.node.unwrap_or_default();
            let network = resource.network.unwrap_or_default();
            let status = SdnStatus::from_str(resource.status.unwrap_or_default().as_str())
                .unwrap_or_default();

            match network_type {
                ClusterResourceNetworkType::Fabric => {
                    Some(PveNetworkResource::Fabric(NetworkFabricResource {
                        id,
                        network,
                        node,
                        status,
                        protocol: resource.protocol.unwrap_or_default(),
                    }))
                }
                ClusterResourceNetworkType::Zone => {
                    Some(PveNetworkResource::Zone(NetworkZoneResource {
                        id,
                        network,
                        node,
                        status,
                        zone_type: resource.zone_type.unwrap_or_default(),
                        legacy: false,
                    }))
                }
                ClusterResourceNetworkType::UnknownEnumValue(variant) => {
                    log::debug!("ignoring unknown network type variant {variant}");
                    None
                }
            }
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
        ClusterResourceType::Sdn => map_pve_sdn(remote, resource).map(Resource::PveNetwork),
        ClusterResourceType::Network => map_pve_network(remote, resource).map(Resource::PveNetwork),
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

#[api(
    // FIXME:: see list-like API calls in resource routers, we probably want more fine-grained
    // checks..
    access: {
        permission: &Permission::Anybody,
    },
    input: {
        properties: {
            "max-age": {
                description: "Maximum age (in seconds) of cached remote resources. If remote is not \
reachable or returns an error for the location, the last value from the cache will be returned in \
any case",
                default: 24*60*60,
                optional: true,
            },
            view: {
                schema: VIEW_ID_SCHEMA,
                optional: true,
            },
        }
    },
)]
/// Get the location info of the selected view (or all remotes)
async fn get_location_info(
    max_age: u64,
    view: Option<String>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<HashMap<String, CachedLocationInfo>, Error> {
    let (remotes_config, _) = pdm_config::remotes::config()?;

    let mut futures = Vec::new();

    let auth_id = rpcenv
        .get_auth_id()
        .context("no authid available")?
        .parse()?;
    let user_info = CachedUserInfo::new()?;

    let allow_all = check_all_remotes_allowed(&user_info, &auth_id, view.as_deref())?;

    let view = views::get_optional_view(view.as_deref())?;

    for (remote_name, remote) in remotes_config {
        if let Some(view) = &view {
            if view.can_skip_remote(&remote_name) {
                continue;
            }
        } else if !allow_all && !check_remote_priv(&user_info, &auth_id, &remote_name) {
            continue;
        }

        let future = async move {
            match crate::location_cache::get_location_info_for_remote(&remote, max_age).await {
                Ok(Some(info)) => Some((remote_name, info)),
                Ok(None) => None,
                Err(err) => {
                    log::debug!("error on getting location data from cache: {err}");
                    None
                }
            }
        };

        futures.push(future);
    }

    let res = join_all(futures).await;
    Ok(res.into_iter().flatten().collect())
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
