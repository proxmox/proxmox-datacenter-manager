use std::convert::Infallible;

use anyhow::{bail, Error};
use serde::{Deserialize, Serialize};

use proxmox_schema::{api, ApiStringFormat, ApiType, EnumEntry, OneOfSchema, Schema, StringSchema};

use super::remotes::{RemoteType, REMOTE_ID_SCHEMA};
use pve_api_types::ClusterResourceNetworkType;

/// High PBS datastore usage threshold
pub const PBS_DATASTORE_HIGH_USAGE_THRESHOLD: f64 = 0.80;

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
    PveNetwork(PveNetworkResource),
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
            Resource::PveNetwork(r) => {
                if let PveNetworkResource::Zone(z) = r {
                    if z.legacy {
                        return format!("sdn/{}/{}", r.node(), r.name());
                    }
                }

                format!("network/{}/{}/{}", r.node(), r.network_type(), r.name())
            }
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
            Resource::PveNetwork(r) => r.id(),
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
            Resource::PveNetwork(r) => r.name(),
            Resource::PbsNode(r) => r.name.as_str(),
            Resource::PbsDatastore(r) => r.name.as_str(),
        }
    }

    pub fn resource_type(&self) -> ResourceType {
        match self {
            Resource::PveStorage(_) => ResourceType::PveStorage,
            Resource::PveQemu(_) => ResourceType::PveQemu,
            Resource::PveLxc(_) => ResourceType::PveLxc,
            Resource::PveNetwork(_) => ResourceType::PveNetwork,
            Resource::PveNode(_) | Resource::PbsNode(_) => ResourceType::Node,
            Resource::PbsDatastore(_) => ResourceType::PbsDatastore,
        }
    }

    pub fn status(&self) -> &str {
        match self {
            Resource::PveStorage(r) => r.status.as_str(),
            Resource::PveQemu(r) => r.status.as_str(),
            Resource::PveLxc(r) => r.status.as_str(),
            Resource::PveNode(r) => r.status.as_str(),
            Resource::PveNetwork(r) => r.status(),
            Resource::PbsNode(r) => {
                if r.uptime > 0 {
                    "online"
                } else {
                    "offline"
                }
            }
            Resource::PbsDatastore(r) => {
                if r.maintenance.is_none() {
                    "online"
                } else {
                    "under-maintenance"
                }
            }
        }
    }

    pub fn properties(&self) -> String {
        let mut properties = Vec::new();
        if let Resource::PbsDatastore(r) = self {
            if let Some(backend_type) = &r.backend_type {
                properties.push(backend_type.to_string());
            }
            if r.backing_device.is_some() {
                properties.push("removable".to_string());
            }
            if r.usage > PBS_DATASTORE_HIGH_USAGE_THRESHOLD {
                properties.push("high-usage".to_string());
            }
        }
        properties.join(",")
    }
}

#[api]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Type of a PDM resource.
pub enum ResourceType {
    /// PVE Storage Resource
    #[serde(rename = "storage")]
    PveStorage,
    /// PVE Qemu Resource
    #[serde(rename = "qemu")]
    PveQemu,
    /// PVE LXC Resource
    #[serde(rename = "lxc")]
    PveLxc,
    /// PVE Network Resource
    #[serde(rename = "network")]
    PveNetwork,
    /// PBS Datastore Resource
    #[serde(rename = "datastore")]
    PbsDatastore,
    /// Node resource
    #[serde(rename = "node")]
    Node,
}

impl ResourceType {
    /// Returns a string representation of the type
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceType::PveStorage => "storage",
            ResourceType::PveQemu => "qemu",
            ResourceType::PveLxc => "lxc",
            ResourceType::PveNetwork => "network",
            ResourceType::PbsDatastore => "datastore",
            ResourceType::Node => "node",
        }
    }
}

impl std::fmt::Display for ResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for ResourceType {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let resource_type = match s {
            "storage" => ResourceType::PveStorage,
            "qemu" => ResourceType::PveQemu,
            "lxc" => ResourceType::PveLxc,
            "network" => ResourceType::PveNetwork,
            "datastore" => ResourceType::PbsDatastore,
            "node" => ResourceType::Node,
            _ => bail!("invalid resource type"),
        };
        Ok(resource_type)
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
    Network(PveNetworkResource),
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
    /// Subscription level
    pub level: String,
}

#[api]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// Storage resource in a PVE remote
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
/// SDN Zone
pub struct NetworkZoneResource {
    /// Resource ID
    pub id: String,
    /// Cluster node name
    pub node: String,
    /// Name of the resource
    pub network: String,
    /// SDN status (available / error)
    pub status: SdnStatus,
    /// Zone type
    pub zone_type: String,
    /// legacy
    pub legacy: bool,
}

impl NetworkZoneResource {
    pub fn status(&self) -> SdnStatus {
        self.status
    }
}

#[api]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// SDN Fabric
pub struct NetworkFabricResource {
    /// Resource ID
    pub id: String,
    /// Cluster node name
    pub node: String,
    /// Name of the resource
    pub network: String,
    /// SDN status (available / error)
    pub status: SdnStatus,
    /// faabric protocol
    pub protocol: String,
}

impl NetworkFabricResource {
    pub fn status(&self) -> SdnStatus {
        self.status
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Copy, Default)]
#[serde(rename_all = "lowercase")]
/// the status of SDN entities
///
/// On the PVE side we have Ok and Available, since SDN Zones have status available if they're ok, but the
/// localnetwork special zone has status ok. This enum merges both into the Available variant.
pub enum SdnStatus {
    Available,
    Error,
    #[serde(other)]
    #[default]
    Unknown,
}

impl std::str::FromStr for SdnStatus {
    type Err = Infallible;

    fn from_str(value: &str) -> Result<Self, Infallible> {
        Ok(match value {
            "ok" | "available" => Self::Available,
            "error" => Self::Error,
            _ => Self::Unknown,
        })
    }
}

proxmox_serde::forward_deserialize_to_from_str!(SdnStatus);
proxmox_serde::forward_display_to_serialize!(SdnStatus);

impl SdnStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Error => "error",
            Self::Unknown => "unknown",
        }
    }
}

impl ApiType for SdnStatus {
    const API_SCHEMA: Schema = StringSchema::new("SDN status").schema();
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "network_type", rename_all = "lowercase")]
/// SDN resource in PDM
pub enum PveNetworkResource {
    Fabric(NetworkFabricResource),
    Zone(NetworkZoneResource),
}

impl ApiType for PveNetworkResource {
    const API_SCHEMA: Schema = OneOfSchema::new(
        "PVE Network resource",
        &(
            "network_type",
            false,
            &StringSchema::new("PVE Network resource type")
                .format(&ApiStringFormat::Enum(&[
                    EnumEntry::new("zone", "An SDN zone."),
                    EnumEntry::new("fabric", "An SDN fabric."),
                ]))
                .schema(),
        ),
        &[
            ("fabric", &NetworkFabricResource::API_SCHEMA),
            ("zone", &NetworkZoneResource::API_SCHEMA),
        ],
    )
    .schema();
}

impl PveNetworkResource {
    pub fn id(&self) -> &str {
        match self {
            Self::Zone(zone) => zone.id.as_str(),
            Self::Fabric(fabric) => fabric.id.as_str(),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Zone(zone) => zone.network.as_str(),
            Self::Fabric(fabric) => fabric.network.as_str(),
        }
    }

    pub fn node(&self) -> &str {
        match self {
            Self::Zone(zone) => zone.node.as_str(),
            Self::Fabric(fabric) => fabric.node.as_str(),
        }
    }

    pub fn status(&self) -> &str {
        match self {
            Self::Zone(zone) => zone.status.as_str(),
            Self::Fabric(fabric) => fabric.status.as_str(),
        }
    }

    pub fn network_type(&self) -> ClusterResourceNetworkType {
        match self {
            Self::Zone(_) => ClusterResourceNetworkType::Zone,
            Self::Fabric(_) => ClusterResourceNetworkType::Fabric,
        }
    }
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
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
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
    /// Datastore contents disk usage
    pub usage: f64,
    /// Datastore maintenance mode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maintenance: Option<String>,
    /// Datastore backing device
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backing_device: Option<String>,
    /// Datastore backend type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_type: Option<String>,
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

    /// Any error that occurred when querying remote resources
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
/// Amount of Proxmox Backup Server datastores with certain state
pub struct PbsDatastoreStatusCount {
    /// Amount of online datastores
    pub online: u64,
    /// Amount of datastores which are in a maintenance mode
    pub under_maintenance: Option<u64>,
    /// Amount of datastores which have high datastore usage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high_usage: Option<u64>,
    /// Amount of datastores in unknown state
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unknown: Option<u64>,
    /// Amount of removable datastores
    #[serde(skip_serializing_if = "Option::is_none")]
    pub removable: Option<u64>,
    /// Amount of datastores with S3 backend
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3_backend: Option<u64>,
}

#[api]
#[derive(Default, Serialize, Deserialize, Clone, PartialEq)]
/// Amount of SDN zones in certain states
pub struct SdnZoneCount {
    /// Amount of available / ok zones
    pub available: u64,
    /// Amount of erroneous sdn zones
    pub error: u64,
    /// Amount of sdn zones with an unknown status
    pub unknown: u64,
}

#[api(
    properties: {
        "failed_remotes_list": {
            type: Array,
            items: {
                type: FailedRemote,
            },
        }
    }
)]
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
    /// Status of storage status
    pub sdn_zones: SdnZoneCount,
    /// Status of PBS Nodes
    pub pbs_nodes: NodeStatusCount,
    /// Status of PBS Datastores
    pub pbs_datastores: PbsDatastoreStatusCount,
    /// List of the failed remotes including type and error
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failed_remotes_list: Vec<FailedRemote>,
}

#[api]
#[derive(Default, Serialize, Deserialize, Clone, PartialEq)]
/// Error information for a failed remote
pub struct FailedRemote {
    /// Name of the failed remote
    pub name: String,
    /// Error that occurred when querying remote resources
    pub error: String,
    /// Type of the failed remote
    pub remote_type: RemoteType,
}

#[api(
    properties: {
        data: {
            type: Array,
            items: {
                type: f64,
                description: "An optional data point.",
            },
        }
    }
)]
#[derive(Default, Serialize, Deserialize, Clone, PartialEq)]
/// An RRD data series.
pub struct ResourceRrdData {
    /// The UNIX epoch of the first data point.
    pub start: u64,
    /// The resolution of the data points.
    pub resolution: u64,
    /// The data points.
    pub data: Vec<Option<f64>>,
}

#[api]
#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// One entry for the list of "top entities"
pub struct TopEntity {
    /// The remote the resource is on
    pub remote: String,
    /// The resource information
    pub resource: Resource,
    /// The rrd data related to this entry.
    pub rrd_data: ResourceRrdData,
}

#[api(
    properties: {
        "guest-cpu": {
            type: Array,
            items: {
                type: TopEntity,
            },
        },
        "node-cpu": {
            type: Array,
            items: {
                type: TopEntity,
            },
        },
        "node-memory": {
            type: Array,
            items: {
                type: TopEntity,
            },
        },
    },
)]
#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// Contains a list for "top entities" for Guest-CPU, Node-CPU and Node-Memory
pub struct TopEntities {
    /// The top entries for Guest CPU
    pub guest_cpu: Vec<TopEntity>,
    /// The top entries for Node CPU
    pub node_cpu: Vec<TopEntity>,
    /// The top entries for Node Memory
    pub node_memory: Vec<TopEntity>,
}

#[derive(PartialEq, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GuestType {
    Qemu,
    Lxc,
}
