use std::sync::Mutex;
use std::time::Instant;
use std::{collections::HashMap, time::Duration};

use anyhow::{Context, Error};
use tokio::{sync::mpsc::Sender, time::MissedTickBehavior};

use proxmox_disks::Disks;
use proxmox_log::{debug, error};
use proxmox_network_api::IpLink;
use proxmox_procfs::pressure::{PressureData, Resource};
use proxmox_sys::fs;
use proxmox_sys::linux::procfs;

use super::rrd_task::RrdStoreRequest;

const HOST_METRIC_COLLECTION_INTERVAL: Duration = Duration::from_secs(10);

/// Task which periodically collects metrics from the PDM host and stores
/// them in the local metrics database.
pub(super) struct LocalMetricCollectionTask {
    metric_data_tx: Sender<RrdStoreRequest>,
}

impl LocalMetricCollectionTask {
    /// Create a new metric collection task.
    pub(super) fn new(metric_data_tx: Sender<RrdStoreRequest>) -> Self {
        Self { metric_data_tx }
    }

    /// Run the metric collection task.
    ///
    /// This function never returns.
    pub(super) async fn run(&mut self) {
        let mut timer = tokio::time::interval(HOST_METRIC_COLLECTION_INTERVAL);
        timer.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            timer.tick().await;
            self.handle_tick().await;
        }
    }

    /// Handle a timer tick.
    async fn handle_tick(&mut self) {
        let stats = match tokio::task::spawn_blocking(collect_host_metrics).await {
            Ok(stats) => stats,
            Err(err) => {
                error!("join error while collecting host stats: {err}");
                return;
            }
        };

        let _ = self
            .metric_data_tx
            .send(RrdStoreRequest::Host {
                timestamp: proxmox_time::epoch_i64(),
                metrics: Box::new(stats),
            })
            .await;
    }
}

/// Container type for various metrics of a PDM host.
pub(super) struct PdmHostMetrics {
    /// CPU statistics from `/proc/stat`.
    pub proc: Option<procfs::ProcFsStat>,
    /// Memory statistics from `/proc/meminfo`.
    pub meminfo: Option<procfs::ProcFsMemInfo>,
    /// System load stats from `/proc/loadavg`.
    pub load: Option<procfs::Loadavg>,
    /// Aggregated network device traffic for all physical NICs.
    pub netstats: Option<NetDevStats>,
    /// Block device stats for the root disk.
    pub root_blockdev_stat: Option<proxmox_disks::BlockDevStat>,
    /// File system usage for the root disk.
    pub root_filesystem_info: Option<fs::FileSystemInformation>,
    /// CPU pressure stall information for the host.
    pub cpu_pressure: Option<PressureData>,
    /// CPU pressure stall information for the host.
    pub memory_pressure: Option<PressureData>,
    /// IO pressure stall information for the host.
    pub io_pressure: Option<PressureData>,
}

/// Aggregated network device traffic for all physical NICs.
pub(super) struct NetDevStats {
    /// Aggregate inbound traffic over all physical NICs in bytes.
    pub netin: u64,
    /// Aggregate outbound traffic over all physical NICs in bytes.
    pub netout: u64,
}

fn collect_host_metrics() -> PdmHostMetrics {
    let proc = procfs::read_proc_stat()
        .inspect_err(|err| error!("failed to read '/proc/stat': {err:#}"))
        .ok();

    let meminfo = procfs::read_meminfo()
        .inspect_err(|err| error!("failed to read '/proc/meminfo': {err:#}"))
        .ok();

    let cpu_pressure = PressureData::read_system(Resource::Cpu)
        .inspect_err(|err| error!("failed to read CPU pressure stall information: {err:#}"))
        .ok();

    let memory_pressure = PressureData::read_system(Resource::Memory)
        .inspect_err(|err| error!("failed to read memory pressure stall information: {err:#}"))
        .ok();

    let io_pressure = PressureData::read_system(Resource::Io)
        .inspect_err(|err| error!("failed to read IO pressure stall information: {err:#}"))
        .ok();

    let load = procfs::read_loadavg()
        .inspect_err(|err| error!("failed to read '/proc/loadavg': {err:#}"))
        .ok();

    let root_blockdev_stat = Disks::new()
        .blockdev_stat_for_path("/")
        .inspect_err(|err| error!("failed to collect blockdev statistics for '/': {err:#}"))
        .ok();

    let root_filesystem_info = proxmox_sys::fs::fs_info("/")
        .inspect_err(|err| {
            error!("failed to query filesystem usage for '/': {err:#}");
        })
        .ok();

    let netstats = collect_netdev_metrics()
        .inspect_err(|err| {
            error!("failed to collect network device statistics: {err:#}");
        })
        .ok();

    PdmHostMetrics {
        proc,
        meminfo,
        load,
        netstats,
        root_blockdev_stat,
        root_filesystem_info,
        cpu_pressure,
        memory_pressure,
        io_pressure,
    }
}

struct NetdevCacheEntry {
    interfaces: HashMap<String, IpLink>,
    timestamp: Instant,
}

const NETWORK_INTERFACE_CACHE_MAX_AGE: Duration = Duration::from_secs(300);
static NETWORK_INTERFACE_CACHE: Mutex<Option<NetdevCacheEntry>> = Mutex::new(None);

fn collect_netdev_metrics() -> Result<NetDevStats, Error> {
    let net_devs = procfs::read_proc_net_dev()?;

    let mut cache = NETWORK_INTERFACE_CACHE.lock().unwrap();

    let now = Instant::now();

    let needs_refresh = match cache.as_ref() {
        Some(entry) => now.duration_since(entry.timestamp) > NETWORK_INTERFACE_CACHE_MAX_AGE,
        None => true,
    };

    if needs_refresh {
        cache.replace({
            debug!("updating cached network devices");

            let interfaces = proxmox_network_api::get_network_interfaces()
                .context("failed to enumerate network devices")?;

            NetdevCacheEntry {
                interfaces,
                timestamp: now,
            }
        });
    }

    // unwrap: at this point we *know* that the Option is Some
    let ip_links = cache.as_ref().unwrap();

    let mut netin = 0;
    let mut netout = 0;

    for net_dev in net_devs {
        if let Some(ip_link) = ip_links.interfaces.get(&net_dev.device) {
            if ip_link.is_physical() {
                netin += net_dev.receive;
                netout += net_dev.send;
            }
        }
    }

    Ok(NetDevStats { netin, netout })
}
