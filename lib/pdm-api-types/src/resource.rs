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
#[derive(Clone, Debug, Deserialize, Serialize)]
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

impl Resource {
    /// Returns the local ID, not a globally unique one, e.g.
    /// `qemu/<vmid>`
    pub fn id(&self) -> String {
        match self {
            Resource::PveStorage(r) => format!("storage/{}", r.storage),
            Resource::PveQemu(r) => format!("qemu/{}", r.vmid),
            Resource::PveLxc(r) => format!("lxc/{}", r.vmid),
            Resource::PveNode(r) => format!("node/{}", r.node),
            Resource::PbsNode(r) => format!("node/{}", r.name),
            Resource::PbsDatastore(r) => r.name.clone(),
        }
    }

    /// Returns the PDM global ID for the resource, e.g.
    /// `remote/<remote-id>/guest/<vmid>`
    pub fn global_id(&self) -> &str {
        match self {
            Resource::PveStorage(r) => r.id.as_str(),
            Resource::PveQemu(r) => r.id.as_str(),
            Resource::PveLxc(r) => r.id.as_str(),
            Resource::PveNode(r) => r.id.as_str(),
            Resource::PbsNode(r) => r.id.as_str(),
            Resource::PbsDatastore(r) => r.id.as_str(),
        }
    }

    /// Returns the "name" of the resource, e.g. the guest name for VMs/Containers or
    /// the hostname for nodes
    pub fn name(&self) -> &str {
        match self {
            Resource::PveStorage(r) => r.storage.as_str(),
            Resource::PveQemu(r) => r.name.as_str(),
            Resource::PveLxc(r) => r.name.as_str(),
            Resource::PveNode(r) => r.node.as_str(),
            Resource::PbsNode(r) => r.name.as_str(),
            Resource::PbsDatastore(r) => r.name.as_str(),
        }
    }
}

#[api]
#[derive(Clone, Debug, Deserialize, Serialize)]
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
#[derive(Clone, Debug, Deserialize, Serialize)]
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
#[derive(Clone, Debug, Deserialize, Serialize)]
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
#[derive(Clone, Debug, Deserialize, Serialize)]
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
#[derive(Clone, Debug, Deserialize, Serialize)]
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
    /// Node name
    pub name: String,
    /// Uptime
    pub uptime: u64,
}

#[api]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
/// PBS datastore resource
pub struct PbsDatastoreResource {
    /// Resource ID
    pub id: String,
    /// Total space
    pub maxdisk: u64,
    /// Use space
    pub disk: u64,
    /// Datastore name
    pub name: String,
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
#[derive(Clone, Debug, Serialize, Deserialize)]
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
