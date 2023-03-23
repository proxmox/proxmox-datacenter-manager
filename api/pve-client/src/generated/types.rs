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
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_isize")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maxcpu: Option<isize>,

    /// Number of available memory in bytes.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_isize")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maxmem: Option<isize>,

    /// Used memory in bytes.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_isize")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mem: Option<isize>,

    /// The cluster node name.
    pub node: String,

    /// The SSL fingerprint for the node certificate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_fingerprint: Option<String>,

    pub status: ClusterNodeIndexResponseStatus,

    /// Node uptime in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_isize")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime: Option<isize>,
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
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_isize")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maxdisk: Option<isize>,

    /// Maximum memory in bytes.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_isize")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maxmem: Option<isize>,

    /// Maximum SWAP memory in bytes.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_isize")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maxswap: Option<isize>,

    /// Container name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    pub status: IsRunning,

    /// The current configured tags, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,

    /// Uptime.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_isize")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime: Option<isize>,

    /// The (unique) ID of the VM.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_usize")]
    pub vmid: usize,
}

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
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_isize")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maxdisk: Option<isize>,

    /// Maximum memory in bytes.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_isize")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maxmem: Option<isize>,

    /// VM name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// PID of running qemu process.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_isize")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<isize>,

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
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_isize")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime: Option<isize>,

    /// The (unique) ID of the VM.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_usize")]
    pub vmid: usize,
}
