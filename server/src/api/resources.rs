use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use anyhow::Error;
use futures::future::join_all;
use futures::FutureExt;

use pbs_api_types::{DataStoreStatusListItem, NodeStatus};
use pdm_api_types::remotes::{Remote, RemoteType};
use pdm_api_types::resource::{
    PbsDatastoreResource, PbsNodeResource, PveLxcResource, PveNodeResource, PveQemuResource,
    PveStorageResource, RemoteResources, Resource, ResourceRrdData, ResourcesStatus,
};
use pdm_api_types::subscription::{
    NodeSubscriptionInfo, RemoteSubscriptionState, RemoteSubscriptions, SubscriptionLevel,
};
use pdm_api_types::PRIV_RESOURCE_AUDIT;
use proxmox_access_control::CachedUserInfo;
use proxmox_router::{list_subdirs_api_method, Permission, Router, RpcEnvironment, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;
use proxmox_subscription::SubscriptionStatus;
use pve_api_types::{ClusterResource, ClusterResourceType};

use crate::connection;

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

#[api(
    // FIXME:: What permissions do we need?
    //access: { permission: &Permission::Anybody, },
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
    search: Option<String>,
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<RemoteResources>, Error> {
    let (remotes_config, _) = pdm_config::remotes::config()?;

    let mut join_handles = Vec::new();

    for (remote_name, remote) in remotes_config {
        let handle = tokio::spawn(async move {
            let (resources, error) = match get_resources_for_remote(remote, max_age).await {
                Ok(resources) => (resources, None),
                Err(error) => (Vec::new(), Some(error.to_string())),
            };

            RemoteResources {
                remote: remote_name,
                resources,
                error,
            }
        });

        join_handles.push(handle);
    }

    let mut remote_resources = Vec::new();
    for handle in join_handles {
        remote_resources.push(handle.await?);
    }

    if let Some(search) = search {
        // FIXME implement more complex filter syntax
        remote_resources.retain_mut(|res| {
            if res.remote.contains(&search) {
                true
            } else {
                res.resources
                    .retain(|res| res.id().contains(&search) || res.name().contains(&search));
                !res.resources.is_empty()
            }
        });
    }

    Ok(remote_resources)
}

#[api(
    // FIXME:: What permissions do we need?
    //access: { permission: &Permission::Anybody, },
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
    let remotes = get_resources(max_age, None, rpcenv).await?;
    let mut counts = ResourcesStatus::default();
    for remote in remotes {
        if remote.error.is_some() {
            counts.failed_remotes += 1;
        } else {
            counts.remotes += 1;
        }
        for resource in remote.resources {
            match resource {
                Resource::PveStorage(r) => match r.status.as_str() {
                    "available" => counts.storages.available += 1,
                    _ => counts.storages.unknown += 1,
                },
                Resource::PveQemu(r) => match r.status.as_str() {
                    "running" => counts.qemu.running += 1,
                    // FIXME: handle templates
                    "stopped" => counts.qemu.stopped += 1,
                    _ => counts.qemu.unknown += 1,
                },
                Resource::PveLxc(r) => match r.status.as_str() {
                    "running" => counts.lxc.running += 1,
                    // FIXME: handle templates
                    "stopped" => counts.lxc.stopped += 1,
                    _ => counts.lxc.unknown += 1,
                },
                Resource::PveNode(r) => match r.status.as_str() {
                    "online" => counts.pve_nodes.online += 1,
                    "offline" => counts.pve_nodes.offline += 1,
                    _ => counts.pve_nodes.unknown += 1,
                },
                // FIXME better status for pbs/datastores
                Resource::PbsNode(_) => counts.pbs_nodes.online += 1,
                Resource::PbsDatastore(_) => counts.pbs_datastores.available += 1,
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
        .check_privs(&auth_id, &["resources"], PRIV_RESOURCE_AUDIT, false)
        .is_ok();

    let check_priv = |remote_name: &str| -> bool {
        user_info
            .check_privs(
                &auth_id,
                &["resources", remote_name],
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
// FIXME: permissions?
#[api]
/// Returns the top X entities regarding the chosen type
async fn get_top_entities() -> Result<Vec<(String, Resource, ResourceRrdData)>, Error> {
    let (remotes_config, _) = pdm_config::remotes::config()?;

    let res = crate::metric_collection::calculate_top(&remotes_config.sections, 10, "cpu_current");
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
        (SubscriptionLevel::Unknown, false) => RemoteSubscriptionState::Mixed,
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
            let client = connection::make_pve_client(&remote)?;

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
            let client = connection::make_pbs_client(&remote)?;

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
async fn get_resources_for_remote(remote: Remote, max_age: u64) -> Result<Vec<Resource>, Error> {
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
async fn fetch_remote_resource(remote: Remote) -> Result<Vec<Resource>, Error> {
    let mut resources = Vec::new();
    let remote_name = remote.id.to_owned();

    match remote.ty {
        RemoteType::Pve => {
            let client = connection::make_pve_client(&remote)?;

            let cluster_resources = client.cluster_resources(None).await?;

            for resource in cluster_resources {
                if let Some(r) = map_pve_resource(&remote_name, resource) {
                    resources.push(r);
                }
            }
        }
        RemoteType::Pbs => {
            let client = connection::make_pbs_client(&remote)?;

            let status = client.node_status().await?;
            resources.push(map_pbs_node_status(&remote_name, status));

            for datastore_usage in client.datastore_usage().await? {
                resources.push(map_pbs_datastore_status(&remote_name, datastore_usage));
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

fn map_pve_resource(remote: &str, resource: ClusterResource) -> Option<Resource> {
    match resource.ty {
        ClusterResourceType::Node => map_pve_node(remote, resource).map(Resource::PveNode),
        ClusterResourceType::Lxc => map_pve_lxc(remote, resource).map(Resource::PveLxc),
        ClusterResourceType::Qemu => map_pve_qemu(remote, resource).map(Resource::PveQemu),
        ClusterResourceType::Storage => map_pve_storage(remote, resource).map(Resource::PveStorage),
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

fn map_pbs_datastore_status(remote: &str, status: DataStoreStatusListItem) -> Resource {
    Resource::PbsDatastore(PbsDatastoreResource {
        id: format!("remote/{remote}/datastore/{}", status.store),
        name: status.store,
        maxdisk: status.total.unwrap_or_default(),
        disk: status.used.unwrap_or_default(),
    })
}
