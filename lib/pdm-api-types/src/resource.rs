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
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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

#[api(
    "id-property": "id",
    "id-schema": {
        type: String,
        description: "Resource ID",
    },
)]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "type")]
pub enum PveResource {
    Storage(PveStorageResource),
    Qemu(PveQemuResource),
    Lxc(PveLxcResource),
    Node(PveNodeResource),
}

#[api(
    properties: {
        tags: {
            type: Array,
            items: {
                description: "A tag",
                type: String,
            }
        }
    }
)]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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
    /// Tags
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub tags: Vec<String>,
    /// Is template
    pub template: bool,
    /// Guest uptime
    pub uptime: u64,
    /// VMID
    pub vmid: u32,
}

#[api(
    properties: {
        tags: {
            type: Array,
            items: {
                description: "A tag",
                type: String,
            }
        }
    }
)]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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
    /// Tags
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    pub tags: Vec<String>,
    /// Is template
    pub template: bool,
    /// Guest uptime
    pub uptime: u64,
    /// VMID
    pub vmid: u32,
}

#[api]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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
    /// Node status
    pub status: String,
}

#[api]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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

#[api]
#[derive(Default, Serialize, Deserialize, Clone, PartialEq)]
/// Amount of guests in certain states
pub struct GuestStatusCount {
    /// Amount of running guests
    pub running: u64,
    /// Amount of stopped guests (excl. templates)
    pub stopped: u64,
    /// Amount of guests that are templates
    pub template: u64,
    /// Amount of guests with an unknown status
    pub unknown: u64,
}

#[api]
#[derive(Default, Serialize, Deserialize, Clone, PartialEq)]
/// Amount of nodes in certain states
pub struct NodeStatusCount {
    /// Amount of online nodes
    pub online: u64,
    /// Amount of offline nodes
    pub offline: u64,
    /// Amount of nodes with an unknown status
    pub unknown: u64,
}

#[api]
#[derive(Default, Serialize, Deserialize, Clone, PartialEq)]
/// Amount of storages in certain states
pub struct StorageStatusCount {
    /// Amount of available storages
    pub available: u64,
    /// Amount of storages with an unknown
    pub unknown: u64,
}

#[api]
#[derive(Default, Serialize, Deserialize, Clone, PartialEq)]
/// Describes the status of seen resources
pub struct ResourcesStatus {
    /// Amount of configured remotes
    pub remotes: u64,
    /// Amount of remotes that returned an error during querying
    pub failed_remotes: u64,
    /// Status of PVE nodes
    pub pve_nodes: NodeStatusCount,
    /// Status of QEMU Guests
    pub qemu: GuestStatusCount,
    /// Status of LXC Guests
    pub lxc: GuestStatusCount,
    /// Status of storage status
    pub storages: StorageStatusCount,
    /// Status of PBS Nodes
    pub pbs_nodes: NodeStatusCount,
    /// Status of PBS Datastores
    pub pbs_datastores: StorageStatusCount,
}
