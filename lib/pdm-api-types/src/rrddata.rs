use proxmox_schema::api;
use serde::{Deserialize, Serialize};

#[api]
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
/// Single point in time with all known data points for VMs.
pub struct QemuDataPoint {
    /// Timestamp (UNIX epoch)
    pub time: u64,
    /// Current CPU utilization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_current: Option<f64>,
    /// Max CPU utiliziation (Number of cores)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_max: Option<f64>,
    /// Disk read rate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_read: Option<f64>,
    /// Disk write rate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_write: Option<f64>,
    /// Total disk size
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_total: Option<f64>,
    /// Total memory size
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_total: Option<f64>,
    /// Currently used memory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_used: Option<f64>,
    /// Inbound network data rate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_in: Option<f64>,
    /// Outboud network data rate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_out: Option<f64>,
    /// Guest uptime
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime: Option<f64>,
}

#[api]
#[derive(Serialize, Deserialize, Default)]
/// Single point in time with all known data points for LXC containers.
pub struct LxcDataPoint {
    /// Timestamp (UNIX epoch)
    pub time: u64,
    /// Current CPU utilization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_current: Option<f64>,
    /// Max CPU utiliziation (Number of cores)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_max: Option<f64>,
    /// Disk read rate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_read: Option<f64>,
    /// Disk write rate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_write: Option<f64>,
    /// Total disk size
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_total: Option<f64>,
    /// Current disk utiliziation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_used: Option<f64>,
    /// Total memory size
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_total: Option<f64>,
    /// Currently used memory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_used: Option<f64>,
    /// Inbound network data rate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_in: Option<f64>,
    /// Outboud network data rate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_out: Option<f64>,
    /// Container uptime
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime: Option<f64>,
}

#[api]
#[derive(Serialize, Deserialize, Default)]
/// Single point in time with all known data points for a PVE host.
pub struct NodeDataPoint {
    /// Timestamp (UNIX epoch)
    pub time: u64,
    /// Current CPU utilization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_current: Option<f64>,
    /// Current IO wait
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_iowait: Option<f64>,
    /// CPU utilization, averaged over the last minute
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_avg1: Option<f64>,
    /// CPU utilization, averaged over the last five minutes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_avg5: Option<f64>,
    /// CPU utilization, averaged over the last fifteen minutes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_avg15: Option<f64>,
    /// Max CPU utiliziation (Number of cores)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_max: Option<f64>,
    /// Total disk size
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_total: Option<f64>,
    /// Disk utiliziation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_used: Option<f64>,
    /// Total swap size
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap_total: Option<f64>,
    /// Currently used swap
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap_used: Option<f64>,
    /// Total memory size
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_total: Option<f64>,
    /// Currently used memory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_used: Option<f64>,
    /// Inbound network data rate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_in: Option<f64>,
    /// Outboud network data rate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_out: Option<f64>,
    /// Container uptime
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime: Option<f64>,
}

#[api]
#[derive(Serialize, Deserialize, Default)]
/// Single point in time with all known data points for a Proxmox Backup Server host.
pub struct PbsNodeDataPoint {
    /// Timestamp (UNIX epoch)
    pub time: u64,
    /// Current CPU utilization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_current: Option<f64>,
    /// Current IO wait
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_iowait: Option<f64>,
    /// CPU utilization, averaged over the last minute
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_avg1: Option<f64>,
    /// CPU utilization, averaged over the last five minutes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_avg5: Option<f64>,
    /// CPU utilization, averaged over the last fifteen minutes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_avg15: Option<f64>,
    /// Total disk size
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_total: Option<f64>,
    /// Disk utiliziation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_used: Option<f64>,
    /// Available disk space
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_available: Option<f64>,
    /// Disk read rate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_read: Option<f64>,
    /// Disk write rate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_write: Option<f64>,
    /// Total swap size
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap_total: Option<f64>,
    /// Currently used swap
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap_used: Option<f64>,
    /// Total memory size
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_total: Option<f64>,
    /// Currently used memory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_used: Option<f64>,
    /// Inbound network data rate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_in: Option<f64>,
    /// Outboud network data rate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_out: Option<f64>,
}

#[api]
#[derive(Serialize, Deserialize, Default)]
/// Single point in time with all known data points for a Proxmox Backup Server datasstore
pub struct PbsDatastoreDataPoint {
    /// Timestamp (UNIX epoch)
    pub time: u64,
    /// Total disk size
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_total: Option<f64>,
    /// Disk utiliziation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_used: Option<f64>,
    /// Available disk space
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_available: Option<f64>,
    /// Disk read rate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_read: Option<f64>,
    /// Disk write rate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_write: Option<f64>,
}
