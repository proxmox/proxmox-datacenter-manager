use std::{collections::HashMap, time::Duration};

use anyhow::{bail, Error};
use pdm_api_types::{remotes::Remote, Authid, ConfigDigest};
use pdm_config::remotes::RemoteConfig;
use proxmox_product_config::ApiLockGuard;
use proxmox_section_config::typed::SectionConfigData;
use pve_api_types::{
    client::PveClient, ClusterMetrics, ClusterMetricsData, ClusterResource, ClusterResourceKind,
    ClusterResourceType, StorageContent,
};
use serde::Deserialize;

use crate::{connection::ClientFactory, pbs_client::PbsClient};

#[derive(Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct FakeRemoteConfig {
    pub nr_of_pve_remotes: u32,
    pub vms_per_pve_remote: u32,
    pub cts_per_pve_remote: u32,
    pub nodes_per_pve_remote: u32,
    pub storages_per_pve_remote: u32,
    pub api_delay: u32,
}

impl RemoteConfig for FakeRemoteConfig {
    fn config(&self) -> Result<(SectionConfigData<Remote>, ConfigDigest), Error> {
        let mut order = Vec::new();
        let mut sections = HashMap::new();

        for i in 0..self.nr_of_pve_remotes {
            let name = format!("pve-{i}");

            sections.insert(
                name.clone(),
                Remote {
                    ty: pdm_api_types::remotes::RemoteType::Pve,
                    id: name.clone(),
                    nodes: Vec::new(),
                    authid: Authid::root_auth_id().clone(),
                    token: "".into(),
                },
            );

            order.push(name);
        }

        let digest = [0u8; 32].into();

        Ok((SectionConfigData { sections, order }, digest))
    }

    fn lock_config(&self) -> Result<ApiLockGuard, Error> {
        unsafe { Ok(proxmox_product_config::create_mocked_lock()) }
    }

    fn save_config(&self, _remotes: &SectionConfigData<Remote>) -> Result<(), Error> {
        Ok(())
    }
}

pub struct FakeClientFactory {
    pub config: FakeRemoteConfig,
}

impl FakeRemoteConfig {
    pub fn from_json_config(path: &str) -> Result<Self, Error> {
        let content = proxmox_sys::fs::file_read_string(path)?;
        let config = serde_json::from_str(&content)?;

        Ok(config)
    }
}

#[async_trait::async_trait]
impl ClientFactory for FakeClientFactory {
    fn make_pve_client(&self, _remote: &Remote) -> Result<Box<dyn PveClient + Send + Sync>, Error> {
        Ok(Box::new(FakePveClient {
            nr_of_vms: self.config.vms_per_pve_remote,
            nr_of_cts: self.config.cts_per_pve_remote,
            nr_of_nodes: self.config.nodes_per_pve_remote,
            nr_of_storages: self.config.storages_per_pve_remote,
            api_delay_ms: self.config.api_delay,
        }))
    }

    fn make_pbs_client(&self, _remote: &Remote) -> Result<Box<PbsClient>, Error> {
        bail!("not implemented")
    }

    async fn make_pve_client_and_login(
        &self,
        _remote: &Remote,
    ) -> Result<Box<dyn PveClient + Send + Sync>, Error> {
        bail!("not implemented")
    }

    async fn make_pbs_client_and_login(&self, _remote: &Remote) -> Result<Box<PbsClient>, Error> {
        bail!("not implemented")
    }
}

struct FakePveClient {
    nr_of_vms: u32,
    nr_of_cts: u32,
    nr_of_nodes: u32,
    nr_of_storages: u32,

    api_delay_ms: u32,
}

#[async_trait::async_trait]
impl PveClient for FakePveClient {
    async fn cluster_resources(
        &self,
        _ty: Option<ClusterResourceKind>,
    ) -> Result<Vec<ClusterResource>, proxmox_client::Error> {
        let mut result = Vec::new();

        let mut vmid = 100;

        for _ in 0..self.nr_of_vms {
            vmid = vmid + 1;
            result.push(ClusterResource {
                cgroup_mode: None,
                content: None,
                cpu: Some(0.1),
                diskread: Some(1034),
                diskwrite: Some(1034),
                disk: Some(42 * 1024 * 1024 * 1024),
                hastate: None,
                id: format!("qemu/{vmid}"),
                level: Some("".into()),
                maxcpu: Some(4.),
                maxdisk: Some(100 * 1024 * 1024),
                maxmem: Some(8 * 1024 * 1024 * 1024),
                mem: Some(3 * 1024 * 1024 * 1024),
                name: Some(format!("vm-{vmid}")),
                netin: Some(1034),
                netout: Some(1034),
                node: Some(format!("node-{}", vmid % self.nr_of_nodes)),
                plugintype: None,
                pool: None,
                status: Some("running".into()),
                storage: None,
                template: Some(false),
                ty: ClusterResourceType::Qemu,
                uptime: Some(1234),
                vmid: Some(vmid),
            });
        }

        for _ in 0..self.nr_of_cts {
            vmid = vmid + 1;
            result.push(ClusterResource {
                cgroup_mode: None,
                content: None,
                cpu: Some(0.1),
                disk: Some(42 * 1024 * 1024 * 1024),
                diskread: Some(1034),
                diskwrite: Some(1034),
                hastate: None,
                id: format!("lxc/{vmid}"),
                level: Some("".into()),
                maxcpu: Some(4.),
                maxdisk: Some(100 * 1024 * 1024),
                maxmem: Some(8 * 1024 * 1024 * 1024),
                mem: Some(3 * 1024 * 1024 * 1024),
                name: Some(format!("ct-{vmid}")),
                netin: Some(1034),
                netout: Some(1034),
                node: Some(format!("node-{}", vmid % self.nr_of_nodes)),
                plugintype: None,
                pool: None,
                status: Some("running".into()),
                storage: None,
                template: Some(false),
                ty: ClusterResourceType::Lxc,
                uptime: Some(1234),
                vmid: Some(vmid),
            });
        }

        for i in 0..self.nr_of_nodes {
            result.push(ClusterResource {
                cgroup_mode: None,
                content: None,
                cpu: Some(0.1),
                disk: Some(42 * 1024 * 1024 * 1024),
                diskread: None,
                diskwrite: None,
                hastate: None,
                id: format!("node/node-{i}"),
                level: Some("".into()),
                maxcpu: Some(16.),
                maxdisk: Some(100 * 1024 * 1024),
                maxmem: Some(8 * 1024 * 1024 * 1024),
                mem: Some(3 * 1024 * 1024 * 1024),
                name: None,
                netin: None,
                netout: None,
                node: Some(format!("node-{i}")),
                plugintype: None,
                pool: None,
                status: Some("online".into()),
                storage: None,
                template: None,
                ty: ClusterResourceType::Node,
                uptime: Some(1234),
                vmid: Some(vmid),
            });
        }

        for i in 0..self.nr_of_storages {
            result.push(ClusterResource {
                cgroup_mode: None,
                content: Some(vec![StorageContent::Images, StorageContent::Rootdir]),
                cpu: None,
                disk: Some(42 * 1024 * 1024 * 1024),
                diskread: None,
                diskwrite: None,
                hastate: None,
                id: format!("storage/node-0/storage-{i}"),
                level: None,
                maxcpu: None,
                maxdisk: Some(100 * 1024 * 1024),
                maxmem: None,
                mem: None,
                name: None,
                netin: None,
                netout: None,
                node: None,
                plugintype: Some("dir".into()),
                pool: None,
                status: Some("available".into()),
                storage: Some(format!("storage-{i}")),
                template: None,
                ty: ClusterResourceType::Storage,
                uptime: None,
                vmid: None,
            });
        }

        tokio::time::sleep(Duration::from_millis(self.api_delay_ms as u64)).await;

        Ok(result)
    }

    /// Retrieve metrics of the cluster.
    async fn cluster_metrics_export(
        &self,
        _history: Option<bool>,
        _local_only: Option<bool>,
        start_time: Option<i64>,
    ) -> Result<ClusterMetrics, proxmox_client::Error> {
        tokio::time::sleep(Duration::from_millis(self.api_delay_ms as u64)).await;

        let now = proxmox_time::epoch_i64();
        let mut time = start_time.unwrap_or(0);
        time = time.max(now - (30 * 60));
        let mut data = Vec::new();

        use pve_api_types::ClusterMetricsDataType::*;

        while time < now {
            let point = |id: &str, metric: &str, timestamp, ty| ClusterMetricsData {
                id: id.into(),
                metric: metric.into(),
                timestamp,
                ty,
                // TODO: Generate random data?
                value: 10.0,
            };

            for i in 0..self.nr_of_nodes {
                let id = format!("node/node-{i}");
                data.push(point(&id, "uptime", time, Gauge));
                data.push(point(&id, "net_in", time, Derive));
                data.push(point(&id, "net_out", time, Derive));
                data.push(point(&id, "cpu_avg1", time, Gauge));
                data.push(point(&id, "cpu_avg5", time, Gauge));
                data.push(point(&id, "cpu_avg15", time, Gauge));
                data.push(point(&id, "cpu_max", time, Gauge));
                data.push(point(&id, "cpu_current", time, Gauge));
                data.push(point(&id, "cpu_iowait", time, Gauge));
                data.push(point(&id, "mem_used", time, Gauge));
                data.push(point(&id, "mem_total", time, Gauge));
                data.push(point(&id, "swap_total", time, Gauge));
                data.push(point(&id, "swap_used", time, Gauge));
                data.push(point(&id, "disk_total", time, Gauge));
                data.push(point(&id, "disk_used", time, Gauge));
            }

            for i in 0..self.nr_of_vms {
                let vmid = 100 + i;
                let id = format!("qemu/{vmid}");
                data.push(point(&id, "uptime", time, Gauge));
                data.push(point(&id, "net_in", time, Derive));
                data.push(point(&id, "net_out", time, Derive));
                data.push(point(&id, "disk_read", time, Derive));
                data.push(point(&id, "disk_write", time, Derive));
                data.push(point(&id, "cpu_max", time, Gauge));
                data.push(point(&id, "cpu_current", time, Gauge));
                data.push(point(&id, "mem_used", time, Gauge));
                data.push(point(&id, "mem_total", time, Gauge));
                data.push(point(&id, "disk_total", time, Gauge));
            }

            for i in 0..self.nr_of_cts {
                let vmid = 100 + self.nr_of_vms + i;
                let id = format!("lxc/{vmid}");
                data.push(point(&id, "uptime", time, Gauge));
                data.push(point(&id, "net_in", time, Derive));
                data.push(point(&id, "net_out", time, Derive));
                data.push(point(&id, "disk_read", time, Derive));
                data.push(point(&id, "disk_write", time, Derive));
                data.push(point(&id, "cpu_max", time, Gauge));
                data.push(point(&id, "cpu_current", time, Gauge));
                data.push(point(&id, "mem_used", time, Gauge));
                data.push(point(&id, "mem_total", time, Gauge));
                data.push(point(&id, "disk_total", time, Gauge));
            }

            for i in 0..self.nr_of_storages {
                let id = format!("storage/node-0/storage-{i}");
                data.push(point(&id, "disk_total", time, Gauge));
                data.push(point(&id, "disk_used", time, Gauge));
            }

            // Advance time by 10 seconds
            time += 10;
        }

        Ok(ClusterMetrics { data })
    }
}
