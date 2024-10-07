use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use anyhow::Error;

use pbs_api_types::{DataStoreStatusListItem, NodeStatus};
use pdm_api_types::remotes::{Remote, RemoteType};
use pdm_api_types::resource::{
    GetResourcesParams, PbsDatastoreResource, PbsNodeResource, PveLxcResource, PveNodeResource,
    PveQemuResource, PveStorageResource, RemoteResources, Resource,
};
use proxmox_router::{Permission, Router, RpcEnvironment};
use proxmox_schema::api;
use proxmox_section_config::typed::SectionConfigData;
use pve_api_types::{ClusterResource, ClusterResourceType};

use crate::pbs_client;

use super::pve;

pub const ROUTER: Router = Router::new().get(&API_METHOD_GET_RESOURCES);

// TODO: What is a sensible default max-age?

/// Default if max-age is not provided
const DEFAULT_MAX_AGE: u64 = 30;

#[api(
    access: {
        // FIXME:: What permissions do we need?
        permission: &Permission::Anybody,
    },
    input: {
        properties: {
            params: {
                type: GetResourcesParams,
                flatten: true,
            }
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
    _rpcenv: &mut dyn RpcEnvironment,
    params: GetResourcesParams,
) -> Result<Vec<RemoteResources>, Error> {
    let (remotes_config, _) = pdm_config::remotes::config()?;

    let max_age = params.max_age.unwrap_or(DEFAULT_MAX_AGE);

    let mut join_handles = Vec::new();

    for name in remotes_config.keys() {
        let remotes = remotes_config.clone();
        let remote_name = name.to_owned();

        let handle = tokio::spawn(async move {
            let (resources, error) =
                match get_resources_for_remote(remotes, &remote_name, max_age).await {
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

    Ok(remote_resources)
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
async fn get_resources_for_remote(
    remotes: SectionConfigData<Remote>,
    remote_name: &str,
    max_age: u64,
) -> Result<Vec<Resource>, Error> {
    if let Some(cached_resource) = get_cached_resources(remote_name, max_age) {
        Ok(cached_resource.resources)
    } else {
        fetch_remote_resource(&remotes, remote_name)
            .await
            .inspect(|resources| {
                let now = proxmox_time::epoch_i64();
                update_cached_resources(remote_name, resources, now);
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
async fn fetch_remote_resource(
    remotes: &SectionConfigData<Remote>,
    remote_name: &str,
) -> Result<Vec<Resource>, Error> {
    let mut resources = Vec::new();

    let remote = remotes
        .get(remote_name)
        .expect("must not fail, remote name comes from iterating the same data structure");

    match remote.ty {
        RemoteType::Pve => {
            let client = pve::connect_to_remote(remotes, remote_name)?;

            let cluster_resources = client.cluster_resources(None).await?;

            for resource in cluster_resources {
                if let Some(r) = map_pve_resource(remote_name, resource) {
                    resources.push(r);
                }
            }
        }
        RemoteType::Pbs => {
            let client = pbs_client::connect_to_remote(remotes, remote_name)?;
            let status = client.node_status().await?;
            resources.push(map_pbs_node_status(remote_name, status));

            for datastore_usage in client.datastore_usage().await? {
                resources.push(map_pbs_datastore_status(remote_name, datastore_usage));
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
        mem: status.memory.used,
        maxmem: status.memory.total,
        uptime: status.uptime,
    })
}

fn map_pbs_datastore_status(remote: &str, status: DataStoreStatusListItem) -> Resource {
    Resource::PbsDatastore(PbsDatastoreResource {
        id: format!("remote/{remote}/datastore/{}", status.store),
        maxdisk: status.total.unwrap_or_default(),
        disk: status.used.unwrap_or_default(),
    })
}
