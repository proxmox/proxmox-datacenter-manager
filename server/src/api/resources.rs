use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use anyhow::Error;

use pbs_api_types::{DataStoreStatusListItem, NodeStatus};
use pdm_api_types::remotes::{Remote, RemoteType};
use pdm_api_types::resource::{
    PbsDatastoreResource, PbsNodeResource, PveLxcResource, PveNodeResource, PveQemuResource,
    PveStorageResource, RemoteResources, Resource, ResourcesStatus,
};
use proxmox_router::{list_subdirs_api_method, Router, RpcEnvironment, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;
use pve_api_types::{ClusterResource, ClusterResourceType};

use crate::connection;

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("list", &Router::new().get(&API_METHOD_GET_RESOURCES)),
    ("status", &Router::new().get(&API_METHOD_GET_STATUS)),
]);

#[api(
    // FIXME:: What permissions do we need?
    //access: { permission: &Permission::Anybody, },
    input: {
        properties: {
            "max-age": {
                description: "Maximum age of cached remote resources.",
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
                description: "Maximum age of cached remote resources.",
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

#[derive(Clone)]
struct CachedResources {
    resources: Vec<Resource>,
    timestamp: i64,
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
        fetch_remote_resource(remote).await.inspect(|resources| {
            let now = proxmox_time::epoch_i64();
            update_cached_resources(&remote_name, resources, now);
        })
    }
}

/// Read cached resource data from the cache
fn get_cached_resources(remote: &str, max_age: u64) -> Option<CachedResources> {
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
        // There *could* be a more recent value in the cache already, e.g.
        // if we had to wait for the lock too long.
        if cached_resource.timestamp < now {
            cache.insert(
                remote.into(),
                CachedResources {
                    timestamp: now,
                    resources: resources.into(),
                },
            );
        }
    } else {
        cache.insert(
            remote.into(),
            CachedResources {
                timestamp: now,
                resources: resources.into(),
            },
        );
    }
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

fn map_pve_resource(remote: &str, resource: ClusterResource) -> Option<Resource> {
    match resource.ty {
        ClusterResourceType::Node => Some(Resource::PveNode(PveNodeResource {
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
        })),
        ClusterResourceType::Lxc => Some(Resource::PveLxc(PveLxcResource {
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
            uptime: resource.uptime.unwrap_or_default() as u64,
            vmid: resource.vmid.unwrap_or_default(),
        })),
        ClusterResourceType::Qemu => Some(Resource::PveQemu(PveQemuResource {
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
            uptime: resource.uptime.unwrap_or_default() as u64,
            vmid: resource.vmid.unwrap_or_default(),
        })),
        ClusterResourceType::Storage => Some(Resource::PveStorage(PveStorageResource {
            disk: resource.disk.unwrap_or_default(),
            maxdisk: resource.maxdisk.unwrap_or_default(),
            id: format!("remote/{remote}/{}", &resource.id),
            storage: resource.storage.unwrap_or_default(),
            node: resource.node.unwrap_or_default(),
            status: resource.status.unwrap_or_default(),
        })),
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
