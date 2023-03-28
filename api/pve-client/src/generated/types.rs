use std::collections::HashMap;

use proxmox_schema::{api, const_regex, ApiStringFormat};

const_regex! {

CLUSTER_NODE_INDEX_RESPONSE_NODE_RE = r##"^([a-zA-Z0-9]([a-zA-Z0-9\-]*[a-zA-Z0-9])?)$"##;

}

#[api(
    properties: {
        node: {
            format: &ApiStringFormat::Pattern(&CLUSTER_NODE_INDEX_RESPONSE_NODE_RE),
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ClusterNodeIndexResponse {
    /// CPU utilization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu: Option<f64>,

    /// Support level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,

    /// Number of available CPUs.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maxcpu: Option<i64>,

    /// Number of available memory in bytes.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maxmem: Option<i64>,

    /// Used memory in bytes.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mem: Option<i64>,

    /// The cluster node name.
    pub node: String,

    /// The SSL fingerprint for the node certificate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_fingerprint: Option<String>,

    pub status: ClusterNodeIndexResponseStatus,

    /// Node uptime in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime: Option<i64>,
}

#[api]
/// Node status.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum ClusterNodeIndexResponseStatus {
    #[serde(rename = "unknown")]
    /// unknown.
    Unknown,
    #[serde(rename = "online")]
    /// online.
    Online,
    #[serde(rename = "offline")]
    /// offline.
    Offline,
}
serde_plain::derive_display_from_serialize!(ClusterNodeIndexResponseStatus);
serde_plain::derive_fromstr_from_deserialize!(ClusterNodeIndexResponseStatus);

const_regex! {

CLUSTER_RESOURCE_NODE_RE = r##"^([a-zA-Z0-9]([a-zA-Z0-9\-]*[a-zA-Z0-9])?)$"##;
CLUSTER_RESOURCE_STORAGE_RE = r##"^[a-zA-Z][a-zA-Z0-9\-_.]*[a-zA-Z0-9]$"##;

}

#[api(
    properties: {
        cpu: {
            minimum: 0.0,
            optional: true,
        },
        disk: {
            minimum: 0,
            optional: true,
        },
        maxcpu: {
            minimum: 0.0,
            optional: true,
        },
        maxdisk: {
            minimum: 0,
            optional: true,
        },
        mem: {
            minimum: 0,
            optional: true,
        },
        node: {
            format: &ApiStringFormat::Pattern(&CLUSTER_RESOURCE_NODE_RE),
            optional: true,
        },
        storage: {
            format: &ApiStringFormat::Pattern(&CLUSTER_RESOURCE_STORAGE_RE),
            optional: true,
        },
        vmid: {
            minimum: 1,
            optional: true,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ClusterResource {
    /// The cgroup mode the node operates under (when type == node).
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "cgroup-mode")]
    pub cgroup_mode: Option<i64>,

    /// Allowed storage content types (when type == storage).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<StorageContent>,

    /// CPU utilization (when type in node,qemu,lxc).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu: Option<f64>,

    /// Used disk space in bytes (when type in storage), used root image spave
    /// for VMs (type in qemu,lxc).
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk: Option<u64>,

    /// HA service status (for HA managed VMs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hastate: Option<String>,

    /// Id.
    pub id: String,

    /// Support level (when type == node).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,

    /// Number of available CPUs (when type in node,qemu,lxc).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maxcpu: Option<f64>,

    /// Storage size in bytes (when type in storage), root image size for VMs
    /// (type in qemu,lxc).
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maxdisk: Option<u64>,

    /// Number of available memory in bytes (when type in node,qemu,lxc).
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maxmem: Option<i64>,

    /// Used memory in bytes (when type in node,qemu,lxc).
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mem: Option<u64>,

    /// Name of the resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// The cluster node name (when type in node,storage,qemu,lxc).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,

    /// More specific type, if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugintype: Option<String>,

    /// The pool name (when type in pool,qemu,lxc).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,

    /// Resource type dependent status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,

    /// The storage identifier (when type == storage).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,

    #[serde(rename = "type")]
    pub ty: ClusterResourceType,

    /// Node uptime in seconds (when type in node,qemu,lxc).
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime: Option<i64>,

    /// The numerical vmid (when type in qemu,lxc).
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vmid: Option<u64>,
}

#[api]
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum ClusterResourceKind {
    #[serde(rename = "vm")]
    /// vm.
    Vm,
    #[serde(rename = "storage")]
    /// storage.
    Storage,
    #[serde(rename = "node")]
    /// node.
    Node,
    #[serde(rename = "sdn")]
    /// sdn.
    Sdn,
}
serde_plain::derive_display_from_serialize!(ClusterResourceKind);
serde_plain::derive_fromstr_from_deserialize!(ClusterResourceKind);

#[api]
/// Resource type.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum ClusterResourceType {
    #[serde(rename = "node")]
    /// node.
    Node,
    #[serde(rename = "storage")]
    /// storage.
    Storage,
    #[serde(rename = "pool")]
    /// pool.
    Pool,
    #[serde(rename = "qemu")]
    /// qemu.
    Qemu,
    #[serde(rename = "lxc")]
    /// lxc.
    Lxc,
    #[serde(rename = "openvz")]
    /// openvz.
    Openvz,
    #[serde(rename = "sdn")]
    /// sdn.
    Sdn,
}
serde_plain::derive_display_from_serialize!(ClusterResourceType);
serde_plain::derive_fromstr_from_deserialize!(ClusterResourceType);

#[api]
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum IsRunning {
    #[serde(rename = "running")]
    /// running.
    Running,
    #[serde(rename = "stopped")]
    /// stopped.
    Stopped,
}
serde_plain::derive_display_from_serialize!(IsRunning);
serde_plain::derive_fromstr_from_deserialize!(IsRunning);

#[api(
    properties: {
        vmid: {
            minimum: 1,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct LxcEntry {
    /// Maximum usable CPUs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpus: Option<f64>,

    /// The current config lock, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<String>,

    /// Root disk size in bytes.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maxdisk: Option<i64>,

    /// Maximum memory in bytes.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maxmem: Option<i64>,

    /// Maximum SWAP memory in bytes.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maxswap: Option<i64>,

    /// Container name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    pub status: IsRunning,

    /// The current configured tags, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,

    /// Uptime.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime: Option<i64>,

    /// The (unique) ID of the VM.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    pub vmid: u64,
}

#[api]
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum StorageContent {
    #[serde(rename = "backup")]
    /// backup.
    Backup,
    #[serde(rename = "images")]
    /// images.
    Images,
    #[serde(rename = "iso")]
    /// iso.
    Iso,
    #[serde(rename = "none")]
    /// none.
    None,
    #[serde(rename = "rootdir")]
    /// rootdir.
    Rootdir,
    #[serde(rename = "snippets")]
    /// snippets.
    Snippets,
    #[serde(rename = "vztmpl")]
    /// vztmpl.
    Vztmpl,
}
serde_plain::derive_display_from_serialize!(StorageContent);
serde_plain::derive_fromstr_from_deserialize!(StorageContent);

#[api]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct VersionResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub console: Option<VersionResponseConsole>,

    /// The current Proxmox VE point release in `x.y` format.
    pub release: String,

    /// The short git revision from which this version was build.
    pub repoid: String,

    /// The full pve-manager package version of this node.
    pub version: String,
}

#[api]
/// The default console viewer to use.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum VersionResponseConsole {
    #[serde(rename = "applet")]
    /// applet.
    Applet,
    #[serde(rename = "vv")]
    /// vv.
    Vv,
    #[serde(rename = "html5")]
    /// html5.
    Html5,
    #[serde(rename = "xtermjs")]
    /// xtermjs.
    Xtermjs,
}
serde_plain::derive_display_from_serialize!(VersionResponseConsole);
serde_plain::derive_fromstr_from_deserialize!(VersionResponseConsole);

#[api(
    properties: {
        vmid: {
            minimum: 1,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct VmEntry {
    /// Maximum usable CPUs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpus: Option<f64>,

    /// The current config lock, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<String>,

    /// Root disk size in bytes.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maxdisk: Option<i64>,

    /// Maximum memory in bytes.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maxmem: Option<i64>,

    /// VM name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// PID of running qemu process.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<i64>,

    /// QEMU QMP agent status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qmpstatus: Option<String>,

    /// The currently running machine type (if running).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "running-machine")]
    pub running_machine: Option<String>,

    /// The currently running QEMU version (if running).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "running-qemu")]
    pub running_qemu: Option<String>,

    pub status: IsRunning,

    /// The current configured tags, if any
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,

    /// Uptime.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime: Option<i64>,

    /// The (unique) ID of the VM.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    pub vmid: u64,
}
