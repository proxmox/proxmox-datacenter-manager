use std::collections::HashMap;

use proxmox_schema::{api, const_regex, ApiStringFormat, ApiType};

const_regex! {

CLUSTER_NODE_INDEX_RESPONSE_NODE_RE = r##"^(?i:[a-z0-9](?i:[a-z0-9\-]*[a-z0-9])?)$"##;

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

CLUSTER_RESOURCE_NODE_RE = r##"^(?i:[a-z0-9](?i:[a-z0-9\-]*[a-z0-9])?)$"##;
CLUSTER_RESOURCE_STORAGE_RE = r##"^(?i:[a-z][a-z0-9\-_.]*[a-z0-9])$"##;

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

    /// Resource id.
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

#[api(
    properties: {
        legacy: {
            default: "cdn",
            optional: true,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct PveQmBoot {
    /// Boot on floppy (a), hard disk (c), CD-ROM (d), or network (n).
    /// Deprecated, use 'order=' instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy: Option<String>,

    /// The guest will attempt to boot from devices in the order they appear
    /// here.
    ///
    /// Disks, optical drives and passed-through storage USB devices will be
    /// directly booted from, NICs will load PXE, and PCIe devices will
    /// either behave like disks (e.g. NVMe) or load an option ROM (e.g.
    /// RAID controller, hardware NIC).
    ///
    /// Note that only devices in this list will be marked as bootable and thus
    /// loaded by the guest firmware (BIOS/UEFI). If you require multiple
    /// disks for booting (e.g. software-raid), you need to specify all of
    /// them here.
    ///
    /// Overrides the deprecated 'legacy=[acdn]*' value when given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
}

#[api(
    properties: {
        meta: {
            format: &ApiStringFormat::VerifyFn(crate::verifiers::verify_volume_id),
            optional: true,
        },
        network: {
            format: &ApiStringFormat::VerifyFn(crate::verifiers::verify_volume_id),
            optional: true,
        },
        user: {
            format: &ApiStringFormat::VerifyFn(crate::verifiers::verify_volume_id),
            optional: true,
        },
        vendor: {
            format: &ApiStringFormat::VerifyFn(crate::verifiers::verify_volume_id),
            optional: true,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct PveQmCicustom {
    /// Specify a custom file containing all meta data passed to the VM via"
    /// 	    ." cloud-init. This is provider specific meaning configdrive2 and
    /// nocloud differ.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<String>,

    /// To pass a custom file containing all network data to the VM via
    /// cloud-init.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,

    /// To pass a custom file containing all user data to the VM via cloud-init.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,

    /// To pass a custom file containing all vendor data to the VM via
    /// cloud-init.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
}

#[api(
    properties: {
        "legacy-igd": {
            default: false,
            optional: true,
        },
        pcie: {
            default: false,
            optional: true,
        },
        rombar: {
            default: true,
            optional: true,
        },
        "x-vga": {
            default: false,
            optional: true,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct PveQmHostpci {
    /// Override PCI device ID visible to guest
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "device-id")]
    pub device_id: Option<String>,

    /// Host PCI device pass through. The PCI ID of a host's PCI device or a
    /// list of PCI virtual functions of the host. HOSTPCIID syntax is:
    ///
    /// 'bus:dev.func' (hexadecimal numbers)
    ///
    /// You can us the 'lspci' command to list existing PCI devices.
    pub host: String,

    /// Pass this device in legacy IGD mode, making it the primary and exclusive
    /// graphics device in the VM. Requires 'pc-i440fx' machine type and VGA set
    /// to 'none'.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "legacy-igd")]
    pub legacy_igd: Option<bool>,

    /// The type of mediated device to use.
    /// An instance of this type will be created on startup of the VM and
    /// will be cleaned up when the VM stops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mdev: Option<String>,

    /// Choose the PCI-express bus (needs the 'q35' machine model).
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pcie: Option<bool>,

    /// Specify whether or not the device's ROM will be visible in the guest's
    /// memory map.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rombar: Option<bool>,

    /// Custom pci device rom filename (must be located in /usr/share/kvm/).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub romfile: Option<String>,

    /// Override PCI subsystem device ID visible to guest
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "sub-device-id")]
    pub sub_device_id: Option<String>,

    /// Override PCI subsystem vendor ID visible to guest
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "sub-vendor-id")]
    pub sub_vendor_id: Option<String>,

    /// Override PCI vendor ID visible to guest
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "vendor-id")]
    pub vendor_id: Option<String>,

    /// Enable vfio-vga device support.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "x-vga")]
    pub x_vga: Option<bool>,
}

const_regex! {

PVE_QM_IDE_MODEL_RE = r##"^[-%a-zA-Z0-9_.!~*'()]*$"##;
PVE_QM_IDE_SERIAL_RE = r##"^[-%a-zA-Z0-9_.!~*'()]*$"##;
PVE_QM_IDE_SIZE_RE = r##"^(\d+(\.\d+)?)([KMGT])?$"##;

}

#[api(
    properties: {
        backup: {
            default: false,
            optional: true,
        },
        bps_max_length: {
            minimum: 1,
            optional: true,
        },
        bps_rd_max_length: {
            minimum: 1,
            optional: true,
        },
        bps_wr_max_length: {
            minimum: 1,
            optional: true,
        },
        detect_zeroes: {
            default: false,
            optional: true,
        },
        file: {
            format: &ApiStringFormat::VerifyFn(crate::verifiers::verify_pve_volume_id_or_qm_path),
        },
        iops_max_length: {
            minimum: 1,
            optional: true,
        },
        iops_rd_max_length: {
            minimum: 1,
            optional: true,
        },
        iops_wr_max_length: {
            minimum: 1,
            optional: true,
        },
        model: {
            format: &ApiStringFormat::Pattern(&PVE_QM_IDE_MODEL_RE),
            max_length: 120,
            optional: true,
        },
        replicate: {
            default: true,
            optional: true,
        },
        serial: {
            format: &ApiStringFormat::Pattern(&PVE_QM_IDE_SERIAL_RE),
            max_length: 60,
            optional: true,
        },
        shared: {
            default: false,
            optional: true,
        },
        size: {
            format: &ApiStringFormat::Pattern(&PVE_QM_IDE_SIZE_RE),
            optional: true,
        },
        snapshot: {
            default: false,
            optional: true,
        },
        ssd: {
            default: false,
            optional: true,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct PveQmIde {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aio: Option<PveQmIdeAio>,

    /// Whether the drive should be included when making backups.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup: Option<bool>,

    /// Maximum r/w speed in bytes per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps: Option<i64>,

    /// Maximum length of I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps_max_length: Option<u64>,

    /// Maximum read speed in bytes per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps_rd: Option<i64>,

    /// Maximum length of read I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps_rd_max_length: Option<u64>,

    /// Maximum write speed in bytes per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps_wr: Option<i64>,

    /// Maximum length of write I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps_wr_max_length: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<PveQmIdeCache>,

    /// Force the drive's physical geometry to have a specific cylinder count.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cyls: Option<i64>,

    /// Controls whether to detect and try to optimize writes of zeroes.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detect_zeroes: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discard: Option<PveQmIdeDiscard>,

    /// The drive's backing volume.
    pub file: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<PveQmIdeFormat>,

    /// Force the drive's physical geometry to have a specific head count.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heads: Option<i64>,

    /// Maximum r/w I/O in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops: Option<i64>,

    /// Maximum unthrottled r/w I/O pool in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_max: Option<i64>,

    /// Maximum length of I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_max_length: Option<u64>,

    /// Maximum read I/O in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_rd: Option<i64>,

    /// Maximum unthrottled read I/O pool in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_rd_max: Option<i64>,

    /// Maximum length of read I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_rd_max_length: Option<u64>,

    /// Maximum write I/O in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_wr: Option<i64>,

    /// Maximum unthrottled write I/O pool in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_wr_max: Option<i64>,

    /// Maximum length of write I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_wr_max_length: Option<u64>,

    /// Maximum r/w speed in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps: Option<f64>,

    /// Maximum unthrottled r/w pool in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps_max: Option<f64>,

    /// Maximum read speed in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps_rd: Option<f64>,

    /// Maximum unthrottled read pool in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps_rd_max: Option<f64>,

    /// Maximum write speed in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps_wr: Option<f64>,

    /// Maximum unthrottled write pool in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps_wr_max: Option<f64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<PveQmIdeMedia>,

    /// The drive's reported model name, url-encoded, up to 40 bytes long.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Whether the drive should considered for replication jobs.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicate: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerror: Option<PveQmIdeRerror>,

    /// Force the drive's physical geometry to have a specific sector count.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secs: Option<i64>,

    /// The drive's reported serial number, url-encoded, up to 20 bytes long.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,

    /// Mark this locally-managed volume as available on all nodes
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared: Option<bool>,

    /// Disk size. This is purely informational and has no effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,

    /// Controls qemu's snapshot mode feature. If activated, changes made to the
    /// disk are temporary and will be discarded when the VM is shutdown.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<bool>,

    /// Whether to expose this drive as an SSD, rather than a rotational hard
    /// disk.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssd: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trans: Option<PveQmIdeTrans>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub werror: Option<PveQmIdeWerror>,

    /// The drive's worldwide name, encoded as 16 bytes hex string, prefixed by
    /// '0x'.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wwn: Option<String>,
}

#[api]
/// AIO type to use.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum PveQmIdeAio {
    #[serde(rename = "native")]
    /// native.
    Native,
    #[serde(rename = "threads")]
    /// threads.
    Threads,
    #[serde(rename = "io_uring")]
    /// io_uring.
    IoUring,
}
serde_plain::derive_display_from_serialize!(PveQmIdeAio);
serde_plain::derive_fromstr_from_deserialize!(PveQmIdeAio);

#[api]
/// The drive's cache mode
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum PveQmIdeCache {
    #[serde(rename = "none")]
    /// none.
    None,
    #[serde(rename = "writethrough")]
    /// writethrough.
    Writethrough,
    #[serde(rename = "writeback")]
    /// writeback.
    Writeback,
    #[serde(rename = "unsafe")]
    /// unsafe.
    Unsafe,
    #[serde(rename = "directsync")]
    /// directsync.
    Directsync,
}
serde_plain::derive_display_from_serialize!(PveQmIdeCache);
serde_plain::derive_fromstr_from_deserialize!(PveQmIdeCache);

#[api]
/// Controls whether to pass discard/trim requests to the underlying storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum PveQmIdeDiscard {
    #[serde(rename = "ignore")]
    /// ignore.
    Ignore,
    #[serde(rename = "on")]
    /// on.
    On,
}
serde_plain::derive_display_from_serialize!(PveQmIdeDiscard);
serde_plain::derive_fromstr_from_deserialize!(PveQmIdeDiscard);

#[api]
/// The drive's backing file's data format.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum PveQmIdeFormat {
    #[serde(rename = "raw")]
    /// raw.
    Raw,
    #[serde(rename = "cow")]
    /// cow.
    Cow,
    #[serde(rename = "qcow")]
    /// qcow.
    Qcow,
    #[serde(rename = "qed")]
    /// qed.
    Qed,
    #[serde(rename = "qcow2")]
    /// qcow2.
    Qcow2,
    #[serde(rename = "vmdk")]
    /// vmdk.
    Vmdk,
    #[serde(rename = "cloop")]
    /// cloop.
    Cloop,
}
serde_plain::derive_display_from_serialize!(PveQmIdeFormat);
serde_plain::derive_fromstr_from_deserialize!(PveQmIdeFormat);

#[api]
/// The drive's media type.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum PveQmIdeMedia {
    #[serde(rename = "cdrom")]
    /// cdrom.
    Cdrom,
    #[serde(rename = "disk")]
    /// disk.
    Disk,
}
serde_plain::derive_display_from_serialize!(PveQmIdeMedia);
serde_plain::derive_fromstr_from_deserialize!(PveQmIdeMedia);

#[api]
/// Read error action.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum PveQmIdeRerror {
    #[serde(rename = "ignore")]
    /// ignore.
    Ignore,
    #[serde(rename = "report")]
    /// report.
    Report,
    #[serde(rename = "stop")]
    /// stop.
    Stop,
}
serde_plain::derive_display_from_serialize!(PveQmIdeRerror);
serde_plain::derive_fromstr_from_deserialize!(PveQmIdeRerror);

#[api]
/// Force disk geometry bios translation mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum PveQmIdeTrans {
    #[serde(rename = "none")]
    /// none.
    None,
    #[serde(rename = "lba")]
    /// lba.
    Lba,
    #[serde(rename = "auto")]
    /// auto.
    Auto,
}
serde_plain::derive_display_from_serialize!(PveQmIdeTrans);
serde_plain::derive_fromstr_from_deserialize!(PveQmIdeTrans);

#[api]
/// Write error action.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum PveQmIdeWerror {
    #[serde(rename = "enospc")]
    /// enospc.
    Enospc,
    #[serde(rename = "ignore")]
    /// ignore.
    Ignore,
    #[serde(rename = "report")]
    /// report.
    Report,
    #[serde(rename = "stop")]
    /// stop.
    Stop,
}
serde_plain::derive_display_from_serialize!(PveQmIdeWerror);
serde_plain::derive_fromstr_from_deserialize!(PveQmIdeWerror);

#[api(
    properties: {
        gw: {
            format: &ApiStringFormat::VerifyFn(crate::verifiers::verify_ipv4),
            optional: true,
        },
        gw6: {
            format: &ApiStringFormat::VerifyFn(crate::verifiers::verify_ipv6),
            optional: true,
        },
        ip: {
            default: "dhcp",
            format: &ApiStringFormat::VerifyFn(crate::verifiers::verify_ipv4_config),
            optional: true,
        },
        ip6: {
            default: "dhcp",
            format: &ApiStringFormat::VerifyFn(crate::verifiers::verify_ipv6_config),
            optional: true,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct PveQmIpconfig {
    /// Default gateway for IPv4 traffic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gw: Option<String>,

    /// Default gateway for IPv6 traffic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gw6: Option<String>,

    /// IPv4 address in CIDR format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,

    /// IPv6 address in CIDR format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip6: Option<String>,
}

#[api(
    properties: {
        base64: {
            default: false,
            optional: true,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct PveQmSmbios1 {
    /// Flag to indicate that the SMBIOS values are base64 encoded
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base64: Option<bool>,

    /// Set SMBIOS1 family string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,

    /// Set SMBIOS1 manufacturer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,

    /// Set SMBIOS1 product ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,

    /// Set SMBIOS1 serial number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,

    /// Set SMBIOS1 SKU string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sku: Option<String>,

    /// Set SMBIOS1 UUID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,

    /// Set SMBIOS1 version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[api]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct PveQmWatchdog {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<PveQmWatchdogAction>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<PveQmWatchdogModel>,
}

#[api]
/// The action to perform if after activation the guest fails to poll the
/// watchdog in time.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum PveQmWatchdogAction {
    #[serde(rename = "reset")]
    /// reset.
    Reset,
    #[serde(rename = "shutdown")]
    /// shutdown.
    Shutdown,
    #[serde(rename = "poweroff")]
    /// poweroff.
    Poweroff,
    #[serde(rename = "pause")]
    /// pause.
    Pause,
    #[serde(rename = "debug")]
    /// debug.
    Debug,
    #[serde(rename = "none")]
    /// none.
    None,
}
serde_plain::derive_display_from_serialize!(PveQmWatchdogAction);
serde_plain::derive_fromstr_from_deserialize!(PveQmWatchdogAction);

#[api]
/// Watchdog type to emulate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum PveQmWatchdogModel {
    #[serde(rename = "i6300esb")]
    /// i6300esb.
    I6300esb,
    #[serde(rename = "ib700")]
    /// ib700.
    Ib700,
}
serde_plain::derive_display_from_serialize!(PveQmWatchdogModel);
serde_plain::derive_fromstr_from_deserialize!(PveQmWatchdogModel);

#[api(
    properties: {
        cputype: {
            default: "kvm64",
            optional: true,
        },
        hidden: {
            default: false,
            optional: true,
        },
        "phys-bits": {
            format: &ApiStringFormat::VerifyFn(crate::verifiers::verify_pve_phys_bits),
            optional: true,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct PveVmCpuConf {
    /// Emulated CPU type. Can be default or custom name (custom model names
    /// must be prefixed with 'custom-').
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cputype: Option<String>,

    /// List of additional CPU flags separated by ';'. Use '+FLAG' to enable,
    /// '-FLAG' to disable a flag. Custom CPU models can specify any flag
    /// supported by QEMU/KVM, VM-specific flags must be from the following set
    /// for security reasons: pcid, spec-ctrl, ibpb, ssbd, virt-ssbd, amd-ssbd,
    /// amd-no-ssb, pdpe1gb, md-clear, hv-tlbflush, hv-evmcs, aes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flags: Option<String>,

    /// Do not identify as a KVM virtual machine.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,

    /// The Hyper-V vendor ID. Some drivers or programs inside Windows guests
    /// need a specific ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "hv-vendor-id")]
    pub hv_vendor_id: Option<String>,

    /// The physical memory address bits that are reported to the guest OS.
    /// Should be smaller or equal to the host's. Set to 'host' to use value
    /// from host CPU, but note that doing so will break live migration to CPUs
    /// with other values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "phys-bits")]
    pub phys_bits: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "reported-model")]
    pub reported_model: Option<PveVmCpuConfReportedModel>,
}

#[api]
/// CPU model and vendor to report to the guest. Must be a QEMU/KVM supported
/// model. Only valid for custom CPU model definitions, default models will
/// always report themselves to the guest OS.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum PveVmCpuConfReportedModel {
    #[serde(rename = "486")]
    /// 486.
    I486,
    #[serde(rename = "athlon")]
    /// athlon.
    Athlon,
    /// Broadwell.
    Broadwell,
    #[serde(rename = "Broadwell-IBRS")]
    /// Broadwell-IBRS.
    BroadwellIbrs,
    #[serde(rename = "Broadwell-noTSX")]
    /// Broadwell-noTSX.
    BroadwellNoTsx,
    #[serde(rename = "Broadwell-noTSX-IBRS")]
    /// Broadwell-noTSX-IBRS.
    BroadwellNoTsxIbrs,
    #[serde(rename = "Cascadelake-Server")]
    /// Cascadelake-Server.
    CascadelakeServer,
    #[serde(rename = "Cascadelake-Server-noTSX")]
    /// Cascadelake-Server-noTSX.
    CascadelakeServerNoTsx,
    /// Conroe.
    Conroe,
    #[serde(rename = "core2duo")]
    /// core2duo.
    Core2duo,
    #[serde(rename = "coreduo")]
    /// coreduo.
    Coreduo,
    #[serde(rename = "EPYC")]
    /// EPYC.
    Epyc,
    #[serde(rename = "EPYC-IBPB")]
    /// EPYC-IBPB.
    EpycIbpb,
    #[serde(rename = "EPYC-Milan")]
    /// EPYC-Milan.
    EpycMilan,
    #[serde(rename = "EPYC-Rome")]
    /// EPYC-Rome.
    EpycRome,
    /// Haswell.
    Haswell,
    #[serde(rename = "Haswell-IBRS")]
    /// Haswell-IBRS.
    HaswellIbrs,
    #[serde(rename = "Haswell-noTSX")]
    /// Haswell-noTSX.
    HaswellNoTsx,
    #[serde(rename = "Haswell-noTSX-IBRS")]
    /// Haswell-noTSX-IBRS.
    HaswellNoTsxIbrs,
    #[serde(rename = "host")]
    /// host.
    Host,
    #[serde(rename = "Icelake-Client")]
    /// Icelake-Client.
    IcelakeClient,
    #[serde(rename = "Icelake-Client-noTSX")]
    /// Icelake-Client-noTSX.
    IcelakeClientNoTsx,
    #[serde(rename = "Icelake-Server")]
    /// Icelake-Server.
    IcelakeServer,
    #[serde(rename = "Icelake-Server-noTSX")]
    /// Icelake-Server-noTSX.
    IcelakeServerNoTsx,
    /// IvyBridge.
    IvyBridge,
    #[serde(rename = "IvyBridge-IBRS")]
    /// IvyBridge-IBRS.
    IvyBridgeIbrs,
    /// KnightsMill.
    KnightsMill,
    #[serde(rename = "kvm32")]
    /// kvm32.
    Kvm32,
    #[serde(rename = "kvm64")]
    /// kvm64.
    Kvm64,
    #[serde(rename = "max")]
    /// max.
    Max,
    /// Nehalem.
    Nehalem,
    #[serde(rename = "Nehalem-IBRS")]
    /// Nehalem-IBRS.
    NehalemIbrs,
    #[serde(rename = "Opteron_G1")]
    /// Opteron_G1.
    OpteronG1,
    #[serde(rename = "Opteron_G2")]
    /// Opteron_G2.
    OpteronG2,
    #[serde(rename = "Opteron_G3")]
    /// Opteron_G3.
    OpteronG3,
    #[serde(rename = "Opteron_G4")]
    /// Opteron_G4.
    OpteronG4,
    #[serde(rename = "Opteron_G5")]
    /// Opteron_G5.
    OpteronG5,
    /// Penryn.
    Penryn,
    #[serde(rename = "pentium")]
    /// pentium.
    Pentium,
    #[serde(rename = "pentium2")]
    /// pentium2.
    Pentium2,
    #[serde(rename = "pentium3")]
    /// pentium3.
    Pentium3,
    #[serde(rename = "phenom")]
    /// phenom.
    Phenom,
    #[serde(rename = "qemu32")]
    /// qemu32.
    Qemu32,
    #[serde(rename = "qemu64")]
    /// qemu64.
    Qemu64,
    /// SandyBridge.
    SandyBridge,
    #[serde(rename = "SandyBridge-IBRS")]
    /// SandyBridge-IBRS.
    SandyBridgeIbrs,
    #[serde(rename = "Skylake-Client")]
    /// Skylake-Client.
    SkylakeClient,
    #[serde(rename = "Skylake-Client-IBRS")]
    /// Skylake-Client-IBRS.
    SkylakeClientIbrs,
    #[serde(rename = "Skylake-Client-noTSX-IBRS")]
    /// Skylake-Client-noTSX-IBRS.
    SkylakeClientNoTsxIbrs,
    #[serde(rename = "Skylake-Server")]
    /// Skylake-Server.
    SkylakeServer,
    #[serde(rename = "Skylake-Server-IBRS")]
    /// Skylake-Server-IBRS.
    SkylakeServerIbrs,
    #[serde(rename = "Skylake-Server-noTSX-IBRS")]
    /// Skylake-Server-noTSX-IBRS.
    SkylakeServerNoTsxIbrs,
    /// Westmere.
    Westmere,
    #[serde(rename = "Westmere-IBRS")]
    /// Westmere-IBRS.
    WestmereIbrs,
}
serde_plain::derive_display_from_serialize!(PveVmCpuConfReportedModel);
serde_plain::derive_fromstr_from_deserialize!(PveVmCpuConfReportedModel);

const_regex! {

QEMU_CONFIG_AFFINITY_RE = r##"^(\s*\d+(-\d+)?\s*)(,\s*\d+(-\d+)?\s*)?$"##;
QEMU_CONFIG_BOOTDISK_RE = r##"^(ide|sata|scsi|virtio|efidisk|tpmstate)\d+$"##;
QEMU_CONFIG_SSHKEYS_RE = r##"^[-%a-zA-Z0-9_.!~*'()]*$"##;
QEMU_CONFIG_TAGS_RE = r##"^(?i)[a-z0-9_][a-z0-9_\-+.]*$"##;
QEMU_CONFIG_VMSTATESTORAGE_RE = r##"^(?i:[a-z][a-z0-9\-_.]*[a-z0-9])$"##;

}

#[api(
    properties: {
        acpi: {
            default: true,
            optional: true,
        },
        affinity: {
            format: &ApiStringFormat::Pattern(&QEMU_CONFIG_AFFINITY_RE),
            optional: true,
        },
        agent: {
            format: &ApiStringFormat::PropertyString(&QemuConfigAgent::API_SCHEMA),
            optional: true,
        },
        audio0: {
            format: &ApiStringFormat::PropertyString(&QemuConfigAudio0::API_SCHEMA),
            optional: true,
        },
        autostart: {
            default: false,
            optional: true,
        },
        balloon: {
            minimum: 0,
            optional: true,
        },
        boot: {
            format: &ApiStringFormat::PropertyString(&PveQmBoot::API_SCHEMA),
            optional: true,
        },
        bootdisk: {
            format: &ApiStringFormat::Pattern(&QEMU_CONFIG_BOOTDISK_RE),
            optional: true,
        },
        cdrom: {
            format: &ApiStringFormat::PropertyString(&PveQmIde::API_SCHEMA),
            optional: true,
            type_text: "<volume>",
        },
        cicustom: {
            format: &ApiStringFormat::PropertyString(&PveQmCicustom::API_SCHEMA),
            optional: true,
        },
        cores: {
            default: 1,
            minimum: 1,
            optional: true,
        },
        cpu: {
            format: &ApiStringFormat::PropertyString(&PveVmCpuConf::API_SCHEMA),
            optional: true,
        },
        cpulimit: {
            default: 0.0,
            maximum: 128.0,
            minimum: 0.0,
            optional: true,
        },
        cpuunits: {
            default: 1024,
            maximum: 262144,
            minimum: 1,
            optional: true,
        },
        description: {
            max_length: 8192,
            optional: true,
        },
        efidisk0: {
            format: &ApiStringFormat::PropertyString(&QemuConfigEfidisk0::API_SCHEMA),
            optional: true,
        },
        freeze: {
            default: false,
            optional: true,
        },
        hookscript: {
            format: &ApiStringFormat::VerifyFn(crate::verifiers::verify_volume_id),
            optional: true,
        },
        hostpci: {
            type: QemuConfigHostpciArray,
        },
        hotplug: {
            default: "network,disk,usb",
            optional: true,
        },
        ide: {
            type: QemuConfigIdeArray,
        },
        ipconfig: {
            type: QemuConfigIpconfigArray,
        },
        ivshmem: {
            format: &ApiStringFormat::PropertyString(&QemuConfigIvshmem::API_SCHEMA),
            optional: true,
        },
        keephugepages: {
            default: false,
            optional: true,
        },
        kvm: {
            default: true,
            optional: true,
        },
        localtime: {
            default: false,
            optional: true,
        },
        machine: {
            max_length: 40,
            optional: true,
        },
        memory: {
            default: 512,
            minimum: 16,
            optional: true,
        },
        migrate_downtime: {
            default: 0.1,
            minimum: 0.0,
            optional: true,
        },
        migrate_speed: {
            default: 0,
            minimum: 0,
            optional: true,
        },
        name: {
            format: &ApiStringFormat::VerifyFn(crate::verifiers::verify_dns_name),
            optional: true,
        },
        nameserver: {
            format: &ApiStringFormat::VerifyFn(crate::verifiers::verify_address),
            optional: true,
        },
        net: {
            type: QemuConfigNetArray,
        },
        numa: {
            default: false,
            optional: true,
        },
        numa_array: {
            type: QemuConfigNumaArray,
        },
        onboot: {
            default: false,
            optional: true,
        },
        parallel: {
            type: QemuConfigParallelArray,
        },
        protection: {
            default: false,
            optional: true,
        },
        reboot: {
            default: true,
            optional: true,
        },
        rng0: {
            format: &ApiStringFormat::PropertyString(&QemuConfigRng0::API_SCHEMA),
            optional: true,
        },
        sata: {
            type: QemuConfigSataArray,
        },
        scsi: {
            type: QemuConfigScsiArray,
        },
        serial: {
            type: QemuConfigSerialArray,
        },
        shares: {
            default: 1000,
            maximum: 50000,
            minimum: 0,
            optional: true,
        },
        smbios1: {
            format: &ApiStringFormat::PropertyString(&PveQmSmbios1::API_SCHEMA),
            max_length: 512,
            optional: true,
        },
        smp: {
            default: 1,
            minimum: 1,
            optional: true,
        },
        sockets: {
            default: 1,
            minimum: 1,
            optional: true,
        },
        spice_enhancements: {
            format: &ApiStringFormat::PropertyString(&QemuConfigSpiceEnhancements::API_SCHEMA),
            optional: true,
        },
        sshkeys: {
            format: &ApiStringFormat::Pattern(&QEMU_CONFIG_SSHKEYS_RE),
            optional: true,
        },
        startdate: {
            default: "now",
            optional: true,
            type_text: "(now | YYYY-MM-DD | YYYY-MM-DDTHH:MM:SS)",
        },
        startup: {
            optional: true,
            type_text: "[[order=]\\d+] [,up=\\d+] [,down=\\d+] ",
        },
        tablet: {
            default: true,
            optional: true,
        },
        tags: {
            format: &ApiStringFormat::Pattern(&QEMU_CONFIG_TAGS_RE),
            optional: true,
        },
        tdf: {
            default: false,
            optional: true,
        },
        template: {
            default: false,
            optional: true,
        },
        tpmstate0: {
            format: &ApiStringFormat::PropertyString(&QemuConfigTpmstate0::API_SCHEMA),
            optional: true,
        },
        unused: {
            type: QemuConfigUnusedArray,
        },
        usb: {
            type: QemuConfigUsbArray,
        },
        vcpus: {
            default: 0,
            minimum: 1,
            optional: true,
        },
        vga: {
            format: &ApiStringFormat::PropertyString(&QemuConfigVga::API_SCHEMA),
            optional: true,
        },
        virtio: {
            type: QemuConfigVirtioArray,
        },
        vmgenid: {
            default: "1 (autogenerated)",
            optional: true,
        },
        vmstatestorage: {
            format: &ApiStringFormat::Pattern(&QEMU_CONFIG_VMSTATESTORAGE_RE),
            optional: true,
        },
        watchdog: {
            format: &ApiStringFormat::PropertyString(&PveQmWatchdog::API_SCHEMA),
            optional: true,
        },
    },
)]
/// The VM configuration.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct QemuConfig {
    /// Enable/disable ACPI.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acpi: Option<bool>,

    /// List of host cores used to execute guest processes, for example:
    /// 0,5,8-11
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affinity: Option<String>,

    /// Enable/disable communication with the QEMU Guest Agent and its
    /// properties.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<QemuConfigArch>,

    /// Arbitrary arguments passed to kvm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,

    /// Configure a audio device, useful in combination with QXL/Spice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio0: Option<String>,

    /// Automatic restart after crash (currently ignored).
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autostart: Option<bool>,

    /// Amount of target RAM for the VM in MiB. Using zero disables the ballon
    /// driver.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balloon: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bios: Option<QemuConfigBios>,

    /// Specify guest boot order. Use the 'order=' sub-property as usage with no
    /// key or 'legacy=' is deprecated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot: Option<String>,

    /// Enable booting from specified disk. Deprecated: Use 'boot:
    /// order=foo;bar' instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bootdisk: Option<String>,

    /// This is an alias for option -ide2
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cdrom: Option<String>,

    /// cloud-init: Specify custom files to replace the automatically generated
    /// ones at start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cicustom: Option<String>,

    /// cloud-init: Password to assign the user. Using this is generally not
    /// recommended. Use ssh keys instead. Also note that older cloud-init
    /// versions do not support hashed passwords.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cipassword: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citype: Option<QemuConfigCitype>,

    /// cloud-init: User name to change ssh keys and password for instead of the
    /// image's configured default user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ciuser: Option<String>,

    /// The number of cores per socket.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cores: Option<u64>,

    /// Emulated CPU type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu: Option<String>,

    /// Limit of CPU usage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpulimit: Option<f64>,

    /// CPU weight for a VM, will be clamped to [1, 10000] in cgroup v2.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u32")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpuunits: Option<u32>,

    /// Description for the VM. Shown in the web-interface VM's summary. This is
    /// saved as comment inside the configuration file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// SHA1 digest of configuration file. This can be used to prevent
    /// concurrent modifications.
    pub digest: String,

    /// Configure a disk for storing EFI vars.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub efidisk0: Option<String>,

    /// Freeze CPU at startup (use 'c' monitor command to start execution).
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freeze: Option<bool>,

    /// Script that will be executed during various steps in the vms lifetime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hookscript: Option<String>,

    /// Map host PCI devices into guest.
    #[serde(flatten)]
    pub hostpci: QemuConfigHostpciArray,

    /// Selectively enable hotplug features. This is a comma separated list of
    /// hotplug features: 'network', 'disk', 'cpu', 'memory', 'usb' and
    /// 'cloudinit'. Use '0' to disable hotplug completely. Using '1' as value
    /// is an alias for the default `network,disk,usb`. USB hotplugging is
    /// possible for guests with machine version >= 7.1 and ostype l26 or
    /// windows > 7.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotplug: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hugepages: Option<QemuConfigHugepages>,

    /// Use volume as IDE hard disk or CD-ROM (n is 0 to 3).
    #[serde(flatten)]
    pub ide: QemuConfigIdeArray,

    /// cloud-init: Specify IP addresses and gateways for the corresponding
    /// interface.
    ///
    /// IP addresses use CIDR notation, gateways are optional but need an IP of
    /// the same type specified.
    ///
    /// The special string 'dhcp' can be used for IP addresses to use DHCP, in
    /// which case no explicit gateway should be provided.
    /// For IPv6 the special string 'auto' can be used to use stateless
    /// autoconfiguration. This requires cloud-init 19.4 or newer.
    ///
    /// If cloud-init is enabled and neither an IPv4 nor an IPv6 address is
    /// specified, it defaults to using dhcp on IPv4.
    #[serde(flatten)]
    pub ipconfig: QemuConfigIpconfigArray,

    /// Inter-VM shared memory. Useful for direct communication between VMs, or
    /// to the host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ivshmem: Option<String>,

    /// Use together with hugepages. If enabled, hugepages will not not be
    /// deleted after VM shutdown and can be used for subsequent starts.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keephugepages: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyboard: Option<QemuConfigKeyboard>,

    /// Enable/disable KVM hardware virtualization.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kvm: Option<bool>,

    /// Set the real time clock (RTC) to local time. This is enabled by default
    /// if the `ostype` indicates a Microsoft Windows OS.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub localtime: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<QemuConfigLock>,

    /// Specifies the QEMU machine type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,

    /// Amount of RAM for the VM in MiB. This is the maximum available memory
    /// when you use the balloon device.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<u64>,

    /// Set maximum tolerated downtime (in seconds) for migrations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migrate_downtime: Option<f64>,

    /// Set maximum speed (in MB/s) for migrations. Value 0 is no limit.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migrate_speed: Option<u64>,

    /// Set a name for the VM. Only used on the configuration web interface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// cloud-init: Sets DNS server IP address for a container. Create will
    /// automatically use the setting from the host if neither searchdomain nor
    /// nameserver are set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nameserver: Option<String>,

    /// Specify network devices.
    #[serde(flatten)]
    pub net: QemuConfigNetArray,

    /// Enable/disable NUMA.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub numa: Option<bool>,

    /// NUMA topology.
    #[serde(flatten)]
    pub numa_array: QemuConfigNumaArray,

    /// Specifies whether a VM will be started during system bootup.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onboot: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ostype: Option<QemuConfigOstype>,

    /// Map host parallel devices (n is 0 to 2).
    #[serde(flatten)]
    pub parallel: QemuConfigParallelArray,

    /// Sets the protection flag of the VM. This will disable the remove VM and
    /// remove disk operations.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protection: Option<bool>,

    /// Allow reboot. If set to '0' the VM exit on reboot.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reboot: Option<bool>,

    /// Configure a VirtIO-based Random Number Generator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rng0: Option<String>,

    /// Use volume as SATA hard disk or CD-ROM (n is 0 to 5).
    #[serde(flatten)]
    pub sata: QemuConfigSataArray,

    /// Use volume as SCSI hard disk or CD-ROM (n is 0 to 30).
    #[serde(flatten)]
    pub scsi: QemuConfigScsiArray,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scsihw: Option<QemuConfigScsihw>,

    /// cloud-init: Sets DNS search domains for a container. Create will
    /// automatically use the setting from the host if neither searchdomain nor
    /// nameserver are set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub searchdomain: Option<String>,

    /// Create a serial device inside the VM (n is 0 to 3)
    #[serde(flatten)]
    pub serial: QemuConfigSerialArray,

    /// Amount of memory shares for auto-ballooning. The larger the number is,
    /// the more memory this VM gets. Number is relative to weights of all other
    /// running VMs. Using zero disables auto-ballooning. Auto-ballooning is
    /// done by pvestatd.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u16")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shares: Option<u16>,

    /// Specify SMBIOS type 1 fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smbios1: Option<String>,

    /// The number of CPUs. Please use option -sockets instead.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smp: Option<u64>,

    /// The number of CPU sockets.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sockets: Option<u64>,

    /// Configure additional enhancements for SPICE.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spice_enhancements: Option<String>,

    /// cloud-init: Setup public SSH keys (one key per line, OpenSSH format).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sshkeys: Option<String>,

    /// Set the initial date of the real time clock. Valid format for date
    /// are:'now' or '2006-06-17T16:01:21' or '2006-06-17'.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startdate: Option<String>,

    /// Startup and shutdown behavior. Order is a non-negative number defining
    /// the general startup order. Shutdown in done with reverse ordering.
    /// Additionally you can set the 'up' or 'down' delay in seconds, which
    /// specifies a delay to wait before the next VM is started or stopped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startup: Option<String>,

    /// Enable/disable the USB tablet device.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tablet: Option<bool>,

    /// Tags of the VM. This is only meta information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,

    /// Enable/disable time drift fix.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tdf: Option<bool>,

    /// Enable/disable Template.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<bool>,

    /// Configure a Disk for storing TPM state. The format is fixed to 'raw'.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tpmstate0: Option<String>,

    /// Reference to unused volumes. This is used internally, and should not be
    /// modified manually.
    #[serde(flatten)]
    pub unused: QemuConfigUnusedArray,

    /// Configure an USB device (n is 0 to 4, for machine version >= 7.1 and
    /// ostype l26 or windows > 7, n can be up to 14).
    #[serde(flatten)]
    pub usb: QemuConfigUsbArray,

    /// Number of hotplugged vcpus.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vcpus: Option<u64>,

    /// Configure the VGA hardware.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vga: Option<String>,

    /// Use volume as VIRTIO hard disk (n is 0 to 15).
    #[serde(flatten)]
    pub virtio: QemuConfigVirtioArray,

    /// Set VM Generation ID. Use '1' to autogenerate on create or update, pass
    /// '0' to disable explicitly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vmgenid: Option<String>,

    /// Default storage for VM state volumes/files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vmstatestorage: Option<String>,

    /// Create a virtual hardware watchdog device.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watchdog: Option<String>,
}
generate_array_field! {
    QemuConfigHostpciArray :
    /// Map host PCI devices into guest.
    String => {
        description: "Map host PCI devices into guest.",
        format: &ApiStringFormat::PropertyString(&PveQmHostpci::API_SCHEMA),
        type: String,
        optional: true,
    }
    hostpci0,
    hostpci1,
    hostpci2,
    hostpci3,
    hostpci4,
    hostpci5,
    hostpci6,
    hostpci7,
    hostpci8,
    hostpci9,
    hostpci10,
    hostpci11,
    hostpci12,
    hostpci13,
    hostpci14,
    hostpci15,
}
generate_array_field! {
    QemuConfigIdeArray :
    /// Use volume as IDE hard disk or CD-ROM (n is 0 to 3).
    String => {
        description: "Use volume as IDE hard disk or CD-ROM (n is 0 to 3).",
        format: &ApiStringFormat::PropertyString(&PveQmIde::API_SCHEMA),
        type: String,
        optional: true,
    }
    ide0,
    ide1,
    ide2,
    ide3,
}
generate_array_field! {
    QemuConfigIpconfigArray :
    /// cloud-init: Specify IP addresses and gateways for the corresponding interface.
    ///
    /// IP addresses use CIDR notation, gateways are optional but need an IP of the same type specified.
    ///
    /// The special string 'dhcp' can be used for IP addresses to use DHCP, in which case no explicit
    /// gateway should be provided.
    /// For IPv6 the special string 'auto' can be used to use stateless autoconfiguration. This requires
    /// cloud-init 19.4 or newer.
    ///
    /// If cloud-init is enabled and neither an IPv4 nor an IPv6 address is specified, it defaults to using
    /// dhcp on IPv4.
    String => {
        description: "cloud-init: Specify IP addresses and gateways for the corresponding interface.

IP addresses use CIDR notation, gateways are optional but need an IP of the same type specified.

The special string 'dhcp' can be used for IP addresses to use DHCP, in which case no explicit
gateway should be provided.
For IPv6 the special string 'auto' can be used to use stateless autoconfiguration. This requires
cloud-init 19.4 or newer.

If cloud-init is enabled and neither an IPv4 nor an IPv6 address is specified, it defaults to using
dhcp on IPv4.
",
        format: &ApiStringFormat::PropertyString(&PveQmIpconfig::API_SCHEMA),
        type: String,
        optional: true,
    }
    ipconfig0,
    ipconfig1,
    ipconfig2,
    ipconfig3,
    ipconfig4,
    ipconfig5,
    ipconfig6,
    ipconfig7,
    ipconfig8,
    ipconfig9,
    ipconfig10,
    ipconfig11,
    ipconfig12,
    ipconfig13,
    ipconfig14,
    ipconfig15,
    ipconfig16,
    ipconfig17,
    ipconfig18,
    ipconfig19,
    ipconfig20,
    ipconfig21,
    ipconfig22,
    ipconfig23,
    ipconfig24,
    ipconfig25,
    ipconfig26,
    ipconfig27,
    ipconfig28,
    ipconfig29,
    ipconfig30,
    ipconfig31,
}
generate_array_field! {
    QemuConfigNetArray :
    /// Specify network devices.
    String => {
        description: "Specify network devices.",
        format: &ApiStringFormat::PropertyString(&QemuConfigNet::API_SCHEMA),
        type: String,
        optional: true,
    }
    net0,
    net1,
    net2,
    net3,
    net4,
    net5,
    net6,
    net7,
    net8,
    net9,
    net10,
    net11,
    net12,
    net13,
    net14,
    net15,
    net16,
    net17,
    net18,
    net19,
    net20,
    net21,
    net22,
    net23,
    net24,
    net25,
    net26,
    net27,
    net28,
    net29,
    net30,
    net31,
}
generate_array_field! {
    QemuConfigNumaArray :
    /// NUMA topology.
    String => {
        description: "NUMA topology.",
        format: &ApiStringFormat::PropertyString(&QemuConfigNuma::API_SCHEMA),
        type: String,
        optional: true,
    }
    numa_array0,
    numa_array1,
    numa_array2,
    numa_array3,
    numa_array4,
    numa_array5,
    numa_array6,
    numa_array7,
}
generate_array_field! {
    QemuConfigParallelArray :
    /// Map host parallel devices (n is 0 to 2).
    String => {
        description: "Map host parallel devices (n is 0 to 2).",
        type: String,
        optional: true,
    }
    parallel0,
    parallel1,
    parallel2,
}
generate_array_field! {
    QemuConfigSataArray :
    /// Use volume as SATA hard disk or CD-ROM (n is 0 to 5).
    String => {
        description: "Use volume as SATA hard disk or CD-ROM (n is 0 to 5).",
        format: &ApiStringFormat::PropertyString(&QemuConfigSata::API_SCHEMA),
        type: String,
        optional: true,
    }
    sata0,
    sata1,
    sata2,
    sata3,
    sata4,
    sata5,
}
generate_array_field! {
    QemuConfigScsiArray :
    /// Use volume as SCSI hard disk or CD-ROM (n is 0 to 30).
    String => {
        description: "Use volume as SCSI hard disk or CD-ROM (n is 0 to 30).",
        format: &ApiStringFormat::PropertyString(&QemuConfigScsi::API_SCHEMA),
        type: String,
        optional: true,
    }
    scsi0,
    scsi1,
    scsi2,
    scsi3,
    scsi4,
    scsi5,
    scsi6,
    scsi7,
    scsi8,
    scsi9,
    scsi10,
    scsi11,
    scsi12,
    scsi13,
    scsi14,
    scsi15,
    scsi16,
    scsi17,
    scsi18,
    scsi19,
    scsi20,
    scsi21,
    scsi22,
    scsi23,
    scsi24,
    scsi25,
    scsi26,
    scsi27,
    scsi28,
    scsi29,
    scsi30,
}
generate_array_field! {
    QemuConfigSerialArray :
    /// Create a serial device inside the VM (n is 0 to 3)
    String => {
        description: "Create a serial device inside the VM (n is 0 to 3)",
        type: String,
        optional: true,
    }
    serial0,
    serial1,
    serial2,
    serial3,
}
generate_array_field! {
    QemuConfigUnusedArray :
    /// Reference to unused volumes. This is used internally, and should not be modified manually.
    String => {
        description: "Reference to unused volumes. This is used internally, and should not be modified manually.",
        format: &ApiStringFormat::PropertyString(&QemuConfigUnused::API_SCHEMA),
        type: String,
        optional: true,
    }
    unused0,
    unused1,
    unused2,
    unused3,
    unused4,
    unused5,
    unused6,
    unused7,
    unused8,
    unused9,
    unused10,
    unused11,
    unused12,
    unused13,
    unused14,
    unused15,
    unused16,
    unused17,
    unused18,
    unused19,
    unused20,
    unused21,
    unused22,
    unused23,
    unused24,
    unused25,
    unused26,
    unused27,
    unused28,
    unused29,
    unused30,
    unused31,
    unused32,
    unused33,
    unused34,
    unused35,
    unused36,
    unused37,
    unused38,
    unused39,
    unused40,
    unused41,
    unused42,
    unused43,
    unused44,
    unused45,
    unused46,
    unused47,
    unused48,
    unused49,
    unused50,
    unused51,
    unused52,
    unused53,
    unused54,
    unused55,
    unused56,
    unused57,
    unused58,
    unused59,
    unused60,
    unused61,
    unused62,
    unused63,
    unused64,
    unused65,
    unused66,
    unused67,
    unused68,
    unused69,
    unused70,
    unused71,
    unused72,
    unused73,
    unused74,
    unused75,
    unused76,
    unused77,
    unused78,
    unused79,
    unused80,
    unused81,
    unused82,
    unused83,
    unused84,
    unused85,
    unused86,
    unused87,
    unused88,
    unused89,
    unused90,
    unused91,
    unused92,
    unused93,
    unused94,
    unused95,
    unused96,
    unused97,
    unused98,
    unused99,
    unused100,
    unused101,
    unused102,
    unused103,
    unused104,
    unused105,
    unused106,
    unused107,
    unused108,
    unused109,
    unused110,
    unused111,
    unused112,
    unused113,
    unused114,
    unused115,
    unused116,
    unused117,
    unused118,
    unused119,
    unused120,
    unused121,
    unused122,
    unused123,
    unused124,
    unused125,
    unused126,
    unused127,
    unused128,
    unused129,
    unused130,
    unused131,
    unused132,
    unused133,
    unused134,
    unused135,
    unused136,
    unused137,
    unused138,
    unused139,
    unused140,
    unused141,
    unused142,
    unused143,
    unused144,
    unused145,
    unused146,
    unused147,
    unused148,
    unused149,
    unused150,
    unused151,
    unused152,
    unused153,
    unused154,
    unused155,
    unused156,
    unused157,
    unused158,
    unused159,
    unused160,
    unused161,
    unused162,
    unused163,
    unused164,
    unused165,
    unused166,
    unused167,
    unused168,
    unused169,
    unused170,
    unused171,
    unused172,
    unused173,
    unused174,
    unused175,
    unused176,
    unused177,
    unused178,
    unused179,
    unused180,
    unused181,
    unused182,
    unused183,
    unused184,
    unused185,
    unused186,
    unused187,
    unused188,
    unused189,
    unused190,
    unused191,
    unused192,
    unused193,
    unused194,
    unused195,
    unused196,
    unused197,
    unused198,
    unused199,
    unused200,
    unused201,
    unused202,
    unused203,
    unused204,
    unused205,
    unused206,
    unused207,
    unused208,
    unused209,
    unused210,
    unused211,
    unused212,
    unused213,
    unused214,
    unused215,
    unused216,
    unused217,
    unused218,
    unused219,
    unused220,
    unused221,
    unused222,
    unused223,
    unused224,
    unused225,
    unused226,
    unused227,
    unused228,
    unused229,
    unused230,
    unused231,
    unused232,
    unused233,
    unused234,
    unused235,
    unused236,
    unused237,
    unused238,
    unused239,
    unused240,
    unused241,
    unused242,
    unused243,
    unused244,
    unused245,
    unused246,
    unused247,
    unused248,
    unused249,
    unused250,
    unused251,
    unused252,
    unused253,
    unused254,
    unused255,
}
generate_array_field! {
    QemuConfigUsbArray :
    /// Configure an USB device (n is 0 to 4, for machine version >= 7.1 and ostype l26 or windows > 7, n can be up to 14).
    String => {
        description: "Configure an USB device (n is 0 to 4, for machine version >= 7.1 and ostype l26 or windows > 7, n can be up to 14).",
        format: &ApiStringFormat::PropertyString(&QemuConfigUsb::API_SCHEMA),
        type: String,
        optional: true,
    }
    usb0,
    usb1,
    usb2,
    usb3,
    usb4,
    usb5,
    usb6,
    usb7,
    usb8,
    usb9,
    usb10,
    usb11,
    usb12,
    usb13,
}
generate_array_field! {
    QemuConfigVirtioArray :
    /// Use volume as VIRTIO hard disk (n is 0 to 15).
    String => {
        description: "Use volume as VIRTIO hard disk (n is 0 to 15).",
        format: &ApiStringFormat::PropertyString(&QemuConfigVirtio::API_SCHEMA),
        type: String,
        optional: true,
    }
    virtio0,
    virtio1,
    virtio2,
    virtio3,
    virtio4,
    virtio5,
    virtio6,
    virtio7,
    virtio8,
    virtio9,
    virtio10,
    virtio11,
    virtio12,
    virtio13,
    virtio14,
    virtio15,
}

#[api(
    properties: {
        enabled: {
            default: false,
        },
        "freeze-fs-on-backup": {
            default: true,
            optional: true,
        },
        fstrim_cloned_disks: {
            default: false,
            optional: true,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct QemuConfigAgent {
    /// Enable/disable communication with a QEMU Guest Agent (QGA) running in
    /// the VM.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    pub enabled: bool,

    /// Freeze/thaw guest filesystems on backup for consistency.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "freeze-fs-on-backup")]
    pub freeze_fs_on_backup: Option<bool>,

    /// Run fstrim after moving a disk or migrating the VM.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fstrim_cloned_disks: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub ty: Option<QemuConfigAgentType>,
}

#[api]
/// Select the agent type
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum QemuConfigAgentType {
    #[serde(rename = "virtio")]
    /// virtio.
    Virtio,
    #[serde(rename = "isa")]
    /// isa.
    Isa,
}
serde_plain::derive_display_from_serialize!(QemuConfigAgentType);
serde_plain::derive_fromstr_from_deserialize!(QemuConfigAgentType);

#[api]
/// Virtual processor architecture. Defaults to the host.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum QemuConfigArch {
    #[serde(rename = "x86_64")]
    /// x86_64.
    X8664,
    #[serde(rename = "aarch64")]
    /// aarch64.
    Aarch64,
}
serde_plain::derive_display_from_serialize!(QemuConfigArch);
serde_plain::derive_fromstr_from_deserialize!(QemuConfigArch);

#[api]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct QemuConfigAudio0 {
    pub device: QemuConfigAudio0Device,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver: Option<QemuConfigAudio0Driver>,
}

#[api]
/// Configure an audio device.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum QemuConfigAudio0Device {
    #[serde(rename = "ich9-intel-hda")]
    /// ich9-intel-hda.
    Ich9IntelHda,
    #[serde(rename = "intel-hda")]
    /// intel-hda.
    IntelHda,
    #[serde(rename = "AC97")]
    /// AC97.
    Ac97,
}
serde_plain::derive_display_from_serialize!(QemuConfigAudio0Device);
serde_plain::derive_fromstr_from_deserialize!(QemuConfigAudio0Device);

#[api]
/// Driver backend for the audio device.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum QemuConfigAudio0Driver {
    #[serde(rename = "spice")]
    /// spice.
    Spice,
    #[serde(rename = "none")]
    /// none.
    None,
}
serde_plain::derive_display_from_serialize!(QemuConfigAudio0Driver);
serde_plain::derive_fromstr_from_deserialize!(QemuConfigAudio0Driver);

#[api]
/// Select BIOS implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum QemuConfigBios {
    #[serde(rename = "seabios")]
    /// seabios.
    Seabios,
    #[serde(rename = "ovmf")]
    /// ovmf.
    Ovmf,
}
serde_plain::derive_display_from_serialize!(QemuConfigBios);
serde_plain::derive_fromstr_from_deserialize!(QemuConfigBios);

#[api]
/// Specifies the cloud-init configuration format. The default depends on the
/// configured operating system type (`ostype`. We use the `nocloud` format for
/// Linux, and `configdrive2` for windows.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum QemuConfigCitype {
    #[serde(rename = "configdrive2")]
    /// configdrive2.
    Configdrive2,
    #[serde(rename = "nocloud")]
    /// nocloud.
    Nocloud,
    #[serde(rename = "opennebula")]
    /// opennebula.
    Opennebula,
}
serde_plain::derive_display_from_serialize!(QemuConfigCitype);
serde_plain::derive_fromstr_from_deserialize!(QemuConfigCitype);

const_regex! {

QEMU_CONFIG_EFIDISK0_SIZE_RE = r##"^(\d+(\.\d+)?)([KMGT])?$"##;

}

#[api(
    properties: {
        file: {
            format: &ApiStringFormat::VerifyFn(crate::verifiers::verify_pve_volume_id_or_qm_path),
        },
        "pre-enrolled-keys": {
            default: false,
            optional: true,
        },
        size: {
            format: &ApiStringFormat::Pattern(&QEMU_CONFIG_EFIDISK0_SIZE_RE),
            optional: true,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct QemuConfigEfidisk0 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub efitype: Option<QemuConfigEfidisk0Efitype>,

    /// The drive's backing volume.
    pub file: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<PveQmIdeFormat>,

    /// Use am EFI vars template with distribution-specific and Microsoft
    /// Standard keys enrolled, if used with 'efitype=4m'. Note that this will
    /// enable Secure Boot by default, though it can still be turned off from
    /// within the VM.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "pre-enrolled-keys")]
    pub pre_enrolled_keys: Option<bool>,

    /// Disk size. This is purely informational and has no effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
}

#[api]
/// Size and type of the OVMF EFI vars. '4m' is newer and recommended, and
/// required for Secure Boot. For backwards compatibility, '2m' is used if not
/// otherwise specified. Ignored for VMs with arch=aarc64 (ARM).
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum QemuConfigEfidisk0Efitype {
    #[serde(rename = "2m")]
    /// 2m.
    Mb2,
    #[serde(rename = "4m")]
    /// 4m.
    Mb4,
}
serde_plain::derive_display_from_serialize!(QemuConfigEfidisk0Efitype);
serde_plain::derive_fromstr_from_deserialize!(QemuConfigEfidisk0Efitype);

#[api]
/// Enable/disable hugepages memory.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum QemuConfigHugepages {
    #[serde(rename = "any")]
    /// any.
    Any,
    #[serde(rename = "2")]
    /// 2.
    Mb2,
    #[serde(rename = "1024")]
    /// 1024.
    Mb1024,
}
serde_plain::derive_display_from_serialize!(QemuConfigHugepages);
serde_plain::derive_fromstr_from_deserialize!(QemuConfigHugepages);

#[api(
    properties: {
        size: {
            minimum: 1,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct QemuConfigIvshmem {
    /// The name of the file. Will be prefixed with 'pve-shm-'. Default is the
    /// VMID. Will be deleted when the VM is stopped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// The size of the file in MB.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    pub size: u64,
}

#[api]
/// Keyboard layout for VNC server. This option is generally not required and is
/// often better handled from within the guest OS.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum QemuConfigKeyboard {
    #[serde(rename = "de")]
    /// de.
    De,
    #[serde(rename = "de-ch")]
    /// de-ch.
    DeCh,
    #[serde(rename = "da")]
    /// da.
    Da,
    #[serde(rename = "en-gb")]
    /// en-gb.
    EnGb,
    #[serde(rename = "en-us")]
    /// en-us.
    EnUs,
    #[serde(rename = "es")]
    /// es.
    Es,
    #[serde(rename = "fi")]
    /// fi.
    Fi,
    #[serde(rename = "fr")]
    /// fr.
    Fr,
    #[serde(rename = "fr-be")]
    /// fr-be.
    FrBe,
    #[serde(rename = "fr-ca")]
    /// fr-ca.
    FrCa,
    #[serde(rename = "fr-ch")]
    /// fr-ch.
    FrCh,
    #[serde(rename = "hu")]
    /// hu.
    Hu,
    #[serde(rename = "is")]
    /// is.
    Is,
    #[serde(rename = "it")]
    /// it.
    It,
    #[serde(rename = "ja")]
    /// ja.
    Ja,
    #[serde(rename = "lt")]
    /// lt.
    Lt,
    #[serde(rename = "mk")]
    /// mk.
    Mk,
    #[serde(rename = "nl")]
    /// nl.
    Nl,
    #[serde(rename = "no")]
    /// no.
    No,
    #[serde(rename = "pl")]
    /// pl.
    Pl,
    #[serde(rename = "pt")]
    /// pt.
    Pt,
    #[serde(rename = "pt-br")]
    /// pt-br.
    PtBr,
    #[serde(rename = "sv")]
    /// sv.
    Sv,
    #[serde(rename = "sl")]
    /// sl.
    Sl,
    #[serde(rename = "tr")]
    /// tr.
    Tr,
}
serde_plain::derive_display_from_serialize!(QemuConfigKeyboard);
serde_plain::derive_fromstr_from_deserialize!(QemuConfigKeyboard);

#[api]
/// Lock/unlock the VM.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum QemuConfigLock {
    #[serde(rename = "backup")]
    /// backup.
    Backup,
    #[serde(rename = "clone")]
    /// clone.
    Clone,
    #[serde(rename = "create")]
    /// create.
    Create,
    #[serde(rename = "migrate")]
    /// migrate.
    Migrate,
    #[serde(rename = "rollback")]
    /// rollback.
    Rollback,
    #[serde(rename = "snapshot")]
    /// snapshot.
    Snapshot,
    #[serde(rename = "snapshot-delete")]
    /// snapshot-delete.
    SnapshotDelete,
    #[serde(rename = "suspending")]
    /// suspending.
    Suspending,
    #[serde(rename = "suspended")]
    /// suspended.
    Suspended,
}
serde_plain::derive_display_from_serialize!(QemuConfigLock);
serde_plain::derive_fromstr_from_deserialize!(QemuConfigLock);

const_regex! {

QEMU_CONFIG_NET_BRIDGE_RE = r##"^[-_.\w\d]+$"##;
QEMU_CONFIG_NET_MACADDR_RE = r##"^(?i)[a-f0-9][02468ace](?::[a-f0-9]{2}){5}$"##;

}

#[api(
    properties: {
        bridge: {
            format: &ApiStringFormat::Pattern(&QEMU_CONFIG_NET_BRIDGE_RE),
            optional: true,
        },
        firewall: {
            default: false,
            optional: true,
        },
        link_down: {
            default: false,
            optional: true,
        },
        macaddr: {
            format: &ApiStringFormat::Pattern(&QEMU_CONFIG_NET_MACADDR_RE),
            optional: true,
        },
        mtu: {
            maximum: 65520,
            minimum: 1,
            optional: true,
        },
        queues: {
            maximum: 64,
            minimum: 0,
            optional: true,
        },
        rate: {
            minimum: 0.0,
            optional: true,
        },
        tag: {
            maximum: 4094,
            minimum: 1,
            optional: true,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct QemuConfigNet {
    /// Bridge to attach the network device to. The Proxmox VE standard bridge
    /// is called 'vmbr0'.
    ///
    /// If you do not specify a bridge, we create a kvm user (NATed) network
    /// device, which provides DHCP and DNS services. The following addresses
    /// are used:
    ///
    ///  10.0.2.2   Gateway
    ///  10.0.2.3   DNS Server
    ///  10.0.2.4   SMB Server
    ///
    /// The DHCP server assign addresses to the guest starting from 10.0.2.15.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge: Option<String>,

    /// Whether this interface should be protected by the firewall.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firewall: Option<bool>,

    /// Whether this interface should be disconnected (like pulling the plug).
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_down: Option<bool>,

    /// MAC address. That address must be unique withing your network. This is
    /// automatically generated if not specified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macaddr: Option<String>,

    pub model: QemuConfigNetModel,

    /// Force MTU, for VirtIO only. Set to '1' to use the bridge MTU
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u16")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtu: Option<u16>,

    /// Number of packet queues to be used on the device.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u8")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queues: Option<u8>,

    /// Rate limit in mbps (megabytes per second) as floating point number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate: Option<f64>,

    /// VLAN tag to apply to packets on this interface.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u16")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<u16>,

    /// VLAN trunks to pass through this interface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trunks: Option<String>,
}

#[api]
/// Network Card Model. The 'virtio' model provides the best performance with
/// very low CPU overhead. If your guest does not support this driver, it is
/// usually best to use 'e1000'.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum QemuConfigNetModel {
    #[serde(rename = "e1000")]
    /// e1000.
    E1000,
    #[serde(rename = "e1000-82540em")]
    /// e1000-82540em.
    E100082540em,
    #[serde(rename = "e1000-82544gc")]
    /// e1000-82544gc.
    E100082544gc,
    #[serde(rename = "e1000-82545em")]
    /// e1000-82545em.
    E100082545em,
    #[serde(rename = "e1000e")]
    /// e1000e.
    E1000e,
    #[serde(rename = "i82551")]
    /// i82551.
    I82551,
    #[serde(rename = "i82557b")]
    /// i82557b.
    I82557b,
    #[serde(rename = "i82559er")]
    /// i82559er.
    I82559er,
    #[serde(rename = "ne2k_isa")]
    /// ne2k_isa.
    Ne2kIsa,
    #[serde(rename = "ne2k_pci")]
    /// ne2k_pci.
    Ne2kPci,
    #[serde(rename = "pcnet")]
    /// pcnet.
    Pcnet,
    #[serde(rename = "rtl8139")]
    /// rtl8139.
    Rtl8139,
    #[serde(rename = "virtio")]
    /// virtio.
    Virtio,
    #[serde(rename = "vmxnet3")]
    /// vmxnet3.
    Vmxnet3,
}
serde_plain::derive_display_from_serialize!(QemuConfigNetModel);
serde_plain::derive_fromstr_from_deserialize!(QemuConfigNetModel);

#[api]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct QemuConfigNuma {
    /// CPUs accessing this NUMA node.
    pub cpus: String,

    /// Host NUMA nodes to use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostnodes: Option<String>,

    /// Amount of memory this NUMA node provides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<f64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<QemuConfigNumaPolicy>,
}

#[api]
/// NUMA allocation policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum QemuConfigNumaPolicy {
    #[serde(rename = "preferred")]
    /// preferred.
    Preferred,
    #[serde(rename = "bind")]
    /// bind.
    Bind,
    #[serde(rename = "interleave")]
    /// interleave.
    Interleave,
}
serde_plain::derive_display_from_serialize!(QemuConfigNumaPolicy);
serde_plain::derive_fromstr_from_deserialize!(QemuConfigNumaPolicy);

#[api]
/// Specify guest operating system.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum QemuConfigOstype {
    #[serde(rename = "other")]
    /// other.
    Other,
    #[serde(rename = "wxp")]
    /// wxp.
    Wxp,
    #[serde(rename = "w2k")]
    /// w2k.
    W2k,
    #[serde(rename = "w2k3")]
    /// w2k3.
    W2k3,
    #[serde(rename = "w2k8")]
    /// w2k8.
    W2k8,
    #[serde(rename = "wvista")]
    /// wvista.
    Wvista,
    #[serde(rename = "win7")]
    /// win7.
    Win7,
    #[serde(rename = "win8")]
    /// win8.
    Win8,
    #[serde(rename = "win10")]
    /// win10.
    Win10,
    #[serde(rename = "win11")]
    /// win11.
    Win11,
    #[serde(rename = "l24")]
    /// l24.
    L24,
    #[serde(rename = "l26")]
    /// l26.
    L26,
    #[serde(rename = "solaris")]
    /// solaris.
    Solaris,
}
serde_plain::derive_display_from_serialize!(QemuConfigOstype);
serde_plain::derive_fromstr_from_deserialize!(QemuConfigOstype);

#[api(
    properties: {
        max_bytes: {
            default: 1024,
            optional: true,
        },
        period: {
            default: 1000,
            optional: true,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct QemuConfigRng0 {
    /// Maximum bytes of entropy allowed to get injected into the guest every
    /// 'period' milliseconds. Prefer a lower value when using '/dev/random' as
    /// source. Use `0` to disable limiting (potentially dangerous!).
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<i64>,

    /// Every 'period' milliseconds the entropy-injection quota is reset,
    /// allowing the guest to retrieve another 'max_bytes' of entropy.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period: Option<i64>,

    pub source: QemuConfigRng0Source,
}

#[api]
/// The file on the host to gather entropy from. In most cases '/dev/urandom'
/// should be preferred over '/dev/random' to avoid entropy-starvation issues on
/// the host. Using urandom does *not* decrease security in any meaningful way,
/// as it's still seeded from real entropy, and the bytes provided will most
/// likely be mixed with real entropy on the guest as well. '/dev/hwrng' can be
/// used to pass through a hardware RNG from the host.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum QemuConfigRng0Source {
    #[serde(rename = "/dev/urandom")]
    /// /dev/urandom.
    DevUrandom,
    #[serde(rename = "/dev/random")]
    /// /dev/random.
    DevRandom,
    #[serde(rename = "/dev/hwrng")]
    /// /dev/hwrng.
    DevHwrng,
}
serde_plain::derive_display_from_serialize!(QemuConfigRng0Source);
serde_plain::derive_fromstr_from_deserialize!(QemuConfigRng0Source);

const_regex! {

QEMU_CONFIG_SATA_SERIAL_RE = r##"^[-%a-zA-Z0-9_.!~*'()]*$"##;
QEMU_CONFIG_SATA_SIZE_RE = r##"^(\d+(\.\d+)?)([KMGT])?$"##;

}

#[api(
    properties: {
        backup: {
            default: false,
            optional: true,
        },
        bps_max_length: {
            minimum: 1,
            optional: true,
        },
        bps_rd_max_length: {
            minimum: 1,
            optional: true,
        },
        bps_wr_max_length: {
            minimum: 1,
            optional: true,
        },
        detect_zeroes: {
            default: false,
            optional: true,
        },
        file: {
            format: &ApiStringFormat::VerifyFn(crate::verifiers::verify_pve_volume_id_or_qm_path),
        },
        iops_max_length: {
            minimum: 1,
            optional: true,
        },
        iops_rd_max_length: {
            minimum: 1,
            optional: true,
        },
        iops_wr_max_length: {
            minimum: 1,
            optional: true,
        },
        replicate: {
            default: true,
            optional: true,
        },
        serial: {
            format: &ApiStringFormat::Pattern(&QEMU_CONFIG_SATA_SERIAL_RE),
            max_length: 60,
            optional: true,
        },
        shared: {
            default: false,
            optional: true,
        },
        size: {
            format: &ApiStringFormat::Pattern(&QEMU_CONFIG_SATA_SIZE_RE),
            optional: true,
        },
        snapshot: {
            default: false,
            optional: true,
        },
        ssd: {
            default: false,
            optional: true,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct QemuConfigSata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aio: Option<PveQmIdeAio>,

    /// Whether the drive should be included when making backups.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup: Option<bool>,

    /// Maximum r/w speed in bytes per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps: Option<i64>,

    /// Maximum length of I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps_max_length: Option<u64>,

    /// Maximum read speed in bytes per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps_rd: Option<i64>,

    /// Maximum length of read I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps_rd_max_length: Option<u64>,

    /// Maximum write speed in bytes per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps_wr: Option<i64>,

    /// Maximum length of write I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps_wr_max_length: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<PveQmIdeCache>,

    /// Force the drive's physical geometry to have a specific cylinder count.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cyls: Option<i64>,

    /// Controls whether to detect and try to optimize writes of zeroes.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detect_zeroes: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discard: Option<PveQmIdeDiscard>,

    /// The drive's backing volume.
    pub file: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<PveQmIdeFormat>,

    /// Force the drive's physical geometry to have a specific head count.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heads: Option<i64>,

    /// Maximum r/w I/O in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops: Option<i64>,

    /// Maximum unthrottled r/w I/O pool in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_max: Option<i64>,

    /// Maximum length of I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_max_length: Option<u64>,

    /// Maximum read I/O in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_rd: Option<i64>,

    /// Maximum unthrottled read I/O pool in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_rd_max: Option<i64>,

    /// Maximum length of read I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_rd_max_length: Option<u64>,

    /// Maximum write I/O in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_wr: Option<i64>,

    /// Maximum unthrottled write I/O pool in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_wr_max: Option<i64>,

    /// Maximum length of write I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_wr_max_length: Option<u64>,

    /// Maximum r/w speed in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps: Option<f64>,

    /// Maximum unthrottled r/w pool in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps_max: Option<f64>,

    /// Maximum read speed in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps_rd: Option<f64>,

    /// Maximum unthrottled read pool in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps_rd_max: Option<f64>,

    /// Maximum write speed in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps_wr: Option<f64>,

    /// Maximum unthrottled write pool in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps_wr_max: Option<f64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<PveQmIdeMedia>,

    /// Whether the drive should considered for replication jobs.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicate: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerror: Option<PveQmIdeRerror>,

    /// Force the drive's physical geometry to have a specific sector count.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secs: Option<i64>,

    /// The drive's reported serial number, url-encoded, up to 20 bytes long.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,

    /// Mark this locally-managed volume as available on all nodes
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared: Option<bool>,

    /// Disk size. This is purely informational and has no effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,

    /// Controls qemu's snapshot mode feature. If activated, changes made to the
    /// disk are temporary and will be discarded when the VM is shutdown.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<bool>,

    /// Whether to expose this drive as an SSD, rather than a rotational hard
    /// disk.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssd: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trans: Option<PveQmIdeTrans>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub werror: Option<PveQmIdeWerror>,

    /// The drive's worldwide name, encoded as 16 bytes hex string, prefixed by
    /// '0x'.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wwn: Option<String>,
}

const_regex! {

QEMU_CONFIG_SCSI_SERIAL_RE = r##"^[-%a-zA-Z0-9_.!~*'()]*$"##;
QEMU_CONFIG_SCSI_SIZE_RE = r##"^(\d+(\.\d+)?)([KMGT])?$"##;

}

#[api(
    properties: {
        backup: {
            default: false,
            optional: true,
        },
        bps_max_length: {
            minimum: 1,
            optional: true,
        },
        bps_rd_max_length: {
            minimum: 1,
            optional: true,
        },
        bps_wr_max_length: {
            minimum: 1,
            optional: true,
        },
        detect_zeroes: {
            default: false,
            optional: true,
        },
        file: {
            format: &ApiStringFormat::VerifyFn(crate::verifiers::verify_pve_volume_id_or_qm_path),
        },
        iops_max_length: {
            minimum: 1,
            optional: true,
        },
        iops_rd_max_length: {
            minimum: 1,
            optional: true,
        },
        iops_wr_max_length: {
            minimum: 1,
            optional: true,
        },
        iothread: {
            default: false,
            optional: true,
        },
        queues: {
            minimum: 2,
            optional: true,
        },
        replicate: {
            default: true,
            optional: true,
        },
        ro: {
            default: false,
            optional: true,
        },
        scsiblock: {
            default: false,
            optional: true,
        },
        serial: {
            format: &ApiStringFormat::Pattern(&QEMU_CONFIG_SCSI_SERIAL_RE),
            max_length: 60,
            optional: true,
        },
        shared: {
            default: false,
            optional: true,
        },
        size: {
            format: &ApiStringFormat::Pattern(&QEMU_CONFIG_SCSI_SIZE_RE),
            optional: true,
        },
        snapshot: {
            default: false,
            optional: true,
        },
        ssd: {
            default: false,
            optional: true,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct QemuConfigScsi {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aio: Option<PveQmIdeAio>,

    /// Whether the drive should be included when making backups.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup: Option<bool>,

    /// Maximum r/w speed in bytes per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps: Option<i64>,

    /// Maximum length of I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps_max_length: Option<u64>,

    /// Maximum read speed in bytes per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps_rd: Option<i64>,

    /// Maximum length of read I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps_rd_max_length: Option<u64>,

    /// Maximum write speed in bytes per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps_wr: Option<i64>,

    /// Maximum length of write I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps_wr_max_length: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<PveQmIdeCache>,

    /// Force the drive's physical geometry to have a specific cylinder count.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cyls: Option<i64>,

    /// Controls whether to detect and try to optimize writes of zeroes.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detect_zeroes: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discard: Option<PveQmIdeDiscard>,

    /// The drive's backing volume.
    pub file: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<PveQmIdeFormat>,

    /// Force the drive's physical geometry to have a specific head count.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heads: Option<i64>,

    /// Maximum r/w I/O in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops: Option<i64>,

    /// Maximum unthrottled r/w I/O pool in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_max: Option<i64>,

    /// Maximum length of I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_max_length: Option<u64>,

    /// Maximum read I/O in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_rd: Option<i64>,

    /// Maximum unthrottled read I/O pool in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_rd_max: Option<i64>,

    /// Maximum length of read I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_rd_max_length: Option<u64>,

    /// Maximum write I/O in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_wr: Option<i64>,

    /// Maximum unthrottled write I/O pool in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_wr_max: Option<i64>,

    /// Maximum length of write I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_wr_max_length: Option<u64>,

    /// Whether to use iothreads for this drive
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iothread: Option<bool>,

    /// Maximum r/w speed in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps: Option<f64>,

    /// Maximum unthrottled r/w pool in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps_max: Option<f64>,

    /// Maximum read speed in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps_rd: Option<f64>,

    /// Maximum unthrottled read pool in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps_rd_max: Option<f64>,

    /// Maximum write speed in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps_wr: Option<f64>,

    /// Maximum unthrottled write pool in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps_wr_max: Option<f64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<PveQmIdeMedia>,

    /// Number of queues.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queues: Option<u64>,

    /// Whether the drive should considered for replication jobs.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicate: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerror: Option<PveQmIdeRerror>,

    /// Whether the drive is read-only.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ro: Option<bool>,

    /// whether to use scsi-block for full passthrough of host block device
    ///
    /// WARNING: can lead to I/O errors in combination with low memory or high
    /// memory fragmentation on host
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scsiblock: Option<bool>,

    /// Force the drive's physical geometry to have a specific sector count.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secs: Option<i64>,

    /// The drive's reported serial number, url-encoded, up to 20 bytes long.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,

    /// Mark this locally-managed volume as available on all nodes
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared: Option<bool>,

    /// Disk size. This is purely informational and has no effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,

    /// Controls qemu's snapshot mode feature. If activated, changes made to the
    /// disk are temporary and will be discarded when the VM is shutdown.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<bool>,

    /// Whether to expose this drive as an SSD, rather than a rotational hard
    /// disk.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssd: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trans: Option<PveQmIdeTrans>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub werror: Option<PveQmIdeWerror>,

    /// The drive's worldwide name, encoded as 16 bytes hex string, prefixed by
    /// '0x'.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wwn: Option<String>,
}

#[api]
/// SCSI controller model
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum QemuConfigScsihw {
    #[serde(rename = "lsi")]
    /// lsi.
    Lsi,
    #[serde(rename = "lsi53c810")]
    /// lsi53c810.
    Lsi53c810,
    #[serde(rename = "virtio-scsi-pci")]
    /// virtio-scsi-pci.
    VirtioScsiPci,
    #[serde(rename = "virtio-scsi-single")]
    /// virtio-scsi-single.
    VirtioScsiSingle,
    #[serde(rename = "megasas")]
    /// megasas.
    Megasas,
    #[serde(rename = "pvscsi")]
    /// pvscsi.
    Pvscsi,
}
serde_plain::derive_display_from_serialize!(QemuConfigScsihw);
serde_plain::derive_fromstr_from_deserialize!(QemuConfigScsihw);

#[api(
    properties: {
        foldersharing: {
            default: false,
            optional: true,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct QemuConfigSpiceEnhancements {
    /// Enable folder sharing via SPICE. Needs Spice-WebDAV daemon installed in
    /// the VM.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foldersharing: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub videostreaming: Option<QemuConfigSpiceEnhancementsVideostreaming>,
}

#[api]
/// Enable video streaming. Uses compression for detected video streams.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum QemuConfigSpiceEnhancementsVideostreaming {
    #[serde(rename = "off")]
    /// off.
    Off,
    #[serde(rename = "all")]
    /// all.
    All,
    #[serde(rename = "filter")]
    /// filter.
    Filter,
}
serde_plain::derive_display_from_serialize!(QemuConfigSpiceEnhancementsVideostreaming);
serde_plain::derive_fromstr_from_deserialize!(QemuConfigSpiceEnhancementsVideostreaming);

const_regex! {

QEMU_CONFIG_TPMSTATE0_SIZE_RE = r##"^(\d+(\.\d+)?)([KMGT])?$"##;

}

#[api(
    properties: {
        file: {
            format: &ApiStringFormat::VerifyFn(crate::verifiers::verify_pve_volume_id_or_qm_path),
        },
        size: {
            format: &ApiStringFormat::Pattern(&QEMU_CONFIG_TPMSTATE0_SIZE_RE),
            optional: true,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct QemuConfigTpmstate0 {
    /// The drive's backing volume.
    pub file: String,

    /// Disk size. This is purely informational and has no effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<QemuConfigTpmstate0Version>,
}

#[api]
/// The TPM interface version. v2.0 is newer and should be preferred. Note that
/// this cannot be changed later on.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum QemuConfigTpmstate0Version {
    #[serde(rename = "v1.2")]
    /// v1.2.
    V1_2,
    #[serde(rename = "v2.0")]
    /// v2.0.
    V2_0,
}
serde_plain::derive_display_from_serialize!(QemuConfigTpmstate0Version);
serde_plain::derive_fromstr_from_deserialize!(QemuConfigTpmstate0Version);

#[api(
    properties: {
        file: {
            format: &ApiStringFormat::VerifyFn(crate::verifiers::verify_volume_id),
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct QemuConfigUnused {
    /// The drive's backing volume.
    pub file: String,
}

#[api(
    properties: {
        usb3: {
            default: false,
            optional: true,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct QemuConfigUsb {
    /// The Host USB device or port or the value 'spice'. HOSTUSBDEVICE syntax
    /// is:
    ///
    ///  'bus-port(.port)*' (decimal numbers) or
    ///  'vendor_id:product_id' (hexadeciaml numbers) or
    ///  'spice'
    ///
    /// You can use the 'lsusb -t' command to list existing usb devices.
    ///
    /// NOTE: This option allows direct access to host hardware. So it is no
    /// longer possible to migrate such machines - use with special care.
    ///
    /// The value 'spice' can be used to add a usb redirection devices for
    /// spice.
    pub host: String,

    /// Specifies whether if given host option is a USB3 device or port. For
    /// modern guests (machine version >= 7.1 and ostype l26 and windows > 7),
    /// this flag is irrelevant (all devices are plugged into a xhci
    /// controller).
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usb3: Option<bool>,
}

#[api(
    properties: {
        memory: {
            maximum: 512,
            minimum: 4,
            optional: true,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct QemuConfigVga {
    /// Sets the VGA memory (in MiB). Has no effect with serial display.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u16")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<u16>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub ty: Option<QemuConfigVgaType>,
}

#[api]
/// Select the VGA type.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum QemuConfigVgaType {
    #[serde(rename = "cirrus")]
    /// cirrus.
    Cirrus,
    #[serde(rename = "qxl")]
    /// qxl.
    Qxl,
    #[serde(rename = "qxl2")]
    /// qxl2.
    Qxl2,
    #[serde(rename = "qxl3")]
    /// qxl3.
    Qxl3,
    #[serde(rename = "qxl4")]
    /// qxl4.
    Qxl4,
    #[serde(rename = "none")]
    /// none.
    None,
    #[serde(rename = "serial0")]
    /// serial0.
    Serial0,
    #[serde(rename = "serial1")]
    /// serial1.
    Serial1,
    #[serde(rename = "serial2")]
    /// serial2.
    Serial2,
    #[serde(rename = "serial3")]
    /// serial3.
    Serial3,
    #[serde(rename = "std")]
    /// std.
    Std,
    #[serde(rename = "virtio")]
    /// virtio.
    Virtio,
    #[serde(rename = "virtio-gl")]
    /// virtio-gl.
    VirtioGl,
    #[serde(rename = "vmware")]
    /// vmware.
    Vmware,
}
serde_plain::derive_display_from_serialize!(QemuConfigVgaType);
serde_plain::derive_fromstr_from_deserialize!(QemuConfigVgaType);

const_regex! {

QEMU_CONFIG_VIRTIO_SERIAL_RE = r##"^[-%a-zA-Z0-9_.!~*'()]*$"##;
QEMU_CONFIG_VIRTIO_SIZE_RE = r##"^(\d+(\.\d+)?)([KMGT])?$"##;

}

#[api(
    properties: {
        backup: {
            default: false,
            optional: true,
        },
        bps_max_length: {
            minimum: 1,
            optional: true,
        },
        bps_rd_max_length: {
            minimum: 1,
            optional: true,
        },
        bps_wr_max_length: {
            minimum: 1,
            optional: true,
        },
        detect_zeroes: {
            default: false,
            optional: true,
        },
        file: {
            format: &ApiStringFormat::VerifyFn(crate::verifiers::verify_pve_volume_id_or_qm_path),
        },
        iops_max_length: {
            minimum: 1,
            optional: true,
        },
        iops_rd_max_length: {
            minimum: 1,
            optional: true,
        },
        iops_wr_max_length: {
            minimum: 1,
            optional: true,
        },
        iothread: {
            default: false,
            optional: true,
        },
        replicate: {
            default: true,
            optional: true,
        },
        ro: {
            default: false,
            optional: true,
        },
        serial: {
            format: &ApiStringFormat::Pattern(&QEMU_CONFIG_VIRTIO_SERIAL_RE),
            max_length: 60,
            optional: true,
        },
        shared: {
            default: false,
            optional: true,
        },
        size: {
            format: &ApiStringFormat::Pattern(&QEMU_CONFIG_VIRTIO_SIZE_RE),
            optional: true,
        },
        snapshot: {
            default: false,
            optional: true,
        },
    },
)]
/// Object.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct QemuConfigVirtio {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aio: Option<PveQmIdeAio>,

    /// Whether the drive should be included when making backups.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup: Option<bool>,

    /// Maximum r/w speed in bytes per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps: Option<i64>,

    /// Maximum length of I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps_max_length: Option<u64>,

    /// Maximum read speed in bytes per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps_rd: Option<i64>,

    /// Maximum length of read I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps_rd_max_length: Option<u64>,

    /// Maximum write speed in bytes per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps_wr: Option<i64>,

    /// Maximum length of write I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps_wr_max_length: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<PveQmIdeCache>,

    /// Force the drive's physical geometry to have a specific cylinder count.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cyls: Option<i64>,

    /// Controls whether to detect and try to optimize writes of zeroes.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detect_zeroes: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discard: Option<PveQmIdeDiscard>,

    /// The drive's backing volume.
    pub file: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<PveQmIdeFormat>,

    /// Force the drive's physical geometry to have a specific head count.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heads: Option<i64>,

    /// Maximum r/w I/O in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops: Option<i64>,

    /// Maximum unthrottled r/w I/O pool in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_max: Option<i64>,

    /// Maximum length of I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_max_length: Option<u64>,

    /// Maximum read I/O in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_rd: Option<i64>,

    /// Maximum unthrottled read I/O pool in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_rd_max: Option<i64>,

    /// Maximum length of read I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_rd_max_length: Option<u64>,

    /// Maximum write I/O in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_wr: Option<i64>,

    /// Maximum unthrottled write I/O pool in operations per second.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_wr_max: Option<i64>,

    /// Maximum length of write I/O bursts in seconds.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_u64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iops_wr_max_length: Option<u64>,

    /// Whether to use iothreads for this drive
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iothread: Option<bool>,

    /// Maximum r/w speed in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps: Option<f64>,

    /// Maximum unthrottled r/w pool in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps_max: Option<f64>,

    /// Maximum read speed in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps_rd: Option<f64>,

    /// Maximum unthrottled read pool in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps_rd_max: Option<f64>,

    /// Maximum write speed in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps_wr: Option<f64>,

    /// Maximum unthrottled write pool in megabytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mbps_wr_max: Option<f64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<PveQmIdeMedia>,

    /// Whether the drive should considered for replication jobs.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicate: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerror: Option<PveQmIdeRerror>,

    /// Whether the drive is read-only.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ro: Option<bool>,

    /// Force the drive's physical geometry to have a specific sector count.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_i64")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secs: Option<i64>,

    /// The drive's reported serial number, url-encoded, up to 20 bytes long.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,

    /// Mark this locally-managed volume as available on all nodes
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared: Option<bool>,

    /// Disk size. This is purely informational and has no effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,

    /// Controls qemu's snapshot mode feature. If activated, changes made to the
    /// disk are temporary and will be discarded when the VM is shutdown.
    #[serde(deserialize_with = "proxmox_login::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trans: Option<PveQmIdeTrans>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub werror: Option<PveQmIdeWerror>,
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
