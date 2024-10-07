use serde::{Deserialize, Serialize};

use proxmox_schema::api;

use super::remotes::REMOTE_ID_SCHEMA;

#[api(
    "id-property": "id",
    "id-schema": {
        type: String,
        description: "Resource ID",
    },
)]
#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "type")]
pub enum Resource {
    PveStorage(PveStorageResource),
    PveQemu(PveQemuResource),
    PveLxc(PveLxcResource),
    PveNode(PveNodeResource),
    PbsNode(PbsNodeResource),
    PbsDatastore(PbsDatastoreResource),
}

#[api]
#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "kebab-case")]
/// QEMU VM resource on a PVE remote
pub struct PveQemuResource {
    /// Current CPU utilization
    pub cpu: f64,
    /// Maximum CPU utilization (Number of CPUs)
    pub maxcpu: f64,
    /// Root disk usage
    pub disk: u64,
    /// Root disk size
    pub maxdisk: u64,
    /// Resource ID
    pub id: String,
    /// System memory
    pub maxmem: u64,
    /// Current memory usage
    pub mem: u64,
    /// Guest name
    pub name: String,
    /// Cluster node name
    pub node: String,
    /// Pool name
    pub pool: String,
    /// Current status
    pub status: String,
    /// Guest uptime
    pub uptime: u64,
    /// VMID
    pub vmid: u32,
}

#[api]
#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "kebab-case")]
/// LXC container resource on a PVE remote
pub struct PveLxcResource {
    /// Current CPU utilization
    pub cpu: f64,
    /// Maximum CPU utilization (Number of CPUs)
    pub maxcpu: f64,
    /// Root disk usage
    pub disk: u64,
    /// Root disk size
    pub maxdisk: u64,
    /// Resource ID
    pub id: String,
    /// System memory
    pub maxmem: u64,
    /// Current memory usage
    pub mem: u64,
    /// Guest name
    pub name: String,
    /// Cluster node name
    pub node: String,
    /// Pool name
    pub pool: String,
    /// Current status
    pub status: String,
    /// Guest uptime
    pub uptime: u64,
    /// VMID
    pub vmid: u32,
}

#[api]
#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "kebab-case")]
/// Node resource in a PVE cluster
pub struct PveNodeResource {
    /// CGroup Mode
    pub cgroup_mode: i64,
    /// Current CPU utilization
    pub cpu: f64,
    /// Maximum CPU utilization (Number of CPUs)
    pub maxcpu: f64,
    /// Resource ID
    pub id: String,
    /// System memory
    pub maxmem: u64,
    /// Current memory usage
    pub mem: u64,
    /// Cluster node name
    pub node: String,
    /// Node uptime
    pub uptime: u64,
}

#[api]
#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "kebab-case")]
/// Storge resource in a PVE remote
pub struct PveStorageResource {
    /// Storage usage
    pub disk: u64,
    /// Storage size
    pub maxdisk: u64,
    /// Resource ID
    pub id: String,
    /// Storage name
    pub storage: String,
    /// Cluster node name
    pub node: String,
    /// Storage status
    pub status: String,
}

#[api]
#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "kebab-case")]
/// PBS node remote resource
pub struct PbsNodeResource {
    /// Current CPU utilization
    pub cpu: f64,
    /// Maximum CPU utilization (Number of CPUs)
    pub maxcpu: f64,
    /// Resource ID
    pub id: String,
    /// Current memory usage
    pub mem: u64,
    /// System memory
    pub maxmem: u64,
    /// Uptime
    pub uptime: u64,
}

#[api]
#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "kebab-case")]
/// PBS datastore resource
pub struct PbsDatastoreResource {
    /// Resource ID
    pub id: String,
    /// Total space
    pub maxdisk: u64,
    /// Use space
    pub disk: u64,
}

#[api(
    properties: {
        remote: {
            schema: REMOTE_ID_SCHEMA,
        },
        resources: {
            type: Array,
            items: {
                type: Resource,
            },
        }
     },
)]
#[derive(Serialize, Deserialize, Clone)]
/// Resource API response
pub struct RemoteResources {
    /// Remote name
    pub remote: String,

    /// Any error that occured when querying remote resources
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Array of resources found at this remote.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<Resource>,
}

#[api]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
/// Parameters for the resources API call
pub struct GetResourcesParams {
    /// Maximum age of cached remote resources
    pub max_age: Option<u64>,
}
