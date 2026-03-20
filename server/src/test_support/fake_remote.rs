use std::{sync::Arc, time::Duration};

use anyhow::{bail, format_err, Error};
use serde::Deserialize;
use serde_json::json;

use pdm_api_types::{remotes::Remote, Authid, ConfigDigest};
use pdm_config::remotes::RemoteConfig;
use proxmox_client::Client;
use proxmox_product_config::ApiLockGuard;
use proxmox_section_config::typed::SectionConfigData;
use pve_api_types::{
    ClusterMetrics, ClusterMetricsData, ClusterNodeIndexResponse, ClusterNodeIndexResponseStatus,
    ClusterResource, ClusterResourceKind, ListTasks, ListTasksResponse, PveUpid,
};

use crate::{
    connection::{ClientFactory, PveClient},
    pbs_client::PbsClient,
};

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
        let mut section_config = SectionConfigData::default();

        for i in 0..self.nr_of_pve_remotes {
            let name = format!("pve-{i}");

            section_config.insert(
                name.clone(),
                Remote {
                    ty: pdm_api_types::remotes::RemoteType::Pve,
                    id: name.clone(),
                    nodes: Vec::new(),
                    authid: Authid::root_auth_id().clone(),
                    token: "".into(),
                    web_url: None,
                },
            );
        }

        let digest = [0u8; 32].into();

        Ok((section_config, digest))
    }

    fn lock_config(&self) -> Result<ApiLockGuard, Error> {
        unsafe { Ok(proxmox_product_config::create_mocked_lock()) }
    }

    fn save_config(&self, _remotes: SectionConfigData<Remote>) -> Result<(), Error> {
        Ok(())
    }

    fn get_secret_token(&self, _remote: &Remote) -> Result<String, Error> {
        Ok(String::new())
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
    fn make_pve_client(&self, _remote: &Remote) -> Result<Arc<PveClient>, Error> {
        Ok(Arc::new(FakePveClient {
            nr_of_vms: self.config.vms_per_pve_remote,
            nr_of_cts: self.config.cts_per_pve_remote,
            nr_of_nodes: self.config.nodes_per_pve_remote,
            nr_of_storages: self.config.storages_per_pve_remote,
            api_delay_ms: self.config.api_delay,
        }))
    }

    fn make_pve_client_with_endpoint(
        &self,
        _remote: &Remote,
        _target_endpoint: Option<&str>,
    ) -> Result<Arc<PveClient>, Error> {
        bail!("not implemented")
    }

    fn make_pbs_client(&self, _remote: &Remote) -> Result<Box<PbsClient>, Error> {
        bail!("not implemented")
    }

    fn make_raw_client(&self, _remote: &Remote) -> Result<Box<Client>, Error> {
        bail!("not implemented")
    }

    async fn make_pve_client_and_login(&self, _remote: &Remote) -> Result<Arc<PveClient>, Error> {
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
impl pve_api_types::client::PveClient for FakePveClient {
    async fn cluster_resources(
        &self,
        _ty: Option<ClusterResourceKind>,
    ) -> Result<Vec<ClusterResource>, proxmox_client::Error> {
        let mut result = Vec::new();

        let mut vmid = 100;

        let disk: u64 = 42 * 1024 * 1024 * 1024;
        let maxdisk: u64 = 100 * 1024 * 1024 * 1024;

        let mem: u64 = 3 * 1024 * 1024 * 1024;
        let memhost: u64 = 4 * 1024 * 1024 * 1024;
        let maxmem: i64 = 8 * 1024 * 1024 * 1024;

        let cpu = 0.1;
        let maxcpu = 4.0;

        let bytejitter = 2.0 * 1024.0 * 1024.0 * 1024.0 * proxmox_time::epoch_f64().sin();

        for _ in 0..self.nr_of_vms {
            vmid += 1;
            let jitter = ((vmid as f64).sin() * bytejitter).round() as i64;
            let val = json!({
                "cpu": cpu,
                "diskread": 1034,
                "diskwrite": 1034,
                "disk": disk.saturating_add_signed(jitter),
                "id": format!("qemu/{vmid}"),
                "level": "",
                "maxcpu": maxcpu,
                "maxdisk": maxdisk,
                "maxmem": maxmem,
                "mem": mem.saturating_add_signed(jitter),
                "memhost": memhost.saturating_add_signed(jitter),
                "name": format!("vm-{vmid}"),
                "netin": 1034,
                "netout": 1034,
                "node": format!("node-{}", vmid % self.nr_of_nodes),
                "status": "running",
                "template": false,
                "type": "qemu",
                "uptime": 1234,
                "vmid": vmid,
            });
            result.push(serde_json::from_value(val).map_err(|err| {
                proxmox_client::Error::Anyhow(format_err!("error on vm {vmid}: {err}"))
            })?);
        }

        for _ in 0..self.nr_of_cts {
            vmid += 1;
            let jitter = ((vmid as f64).sin() * bytejitter).round() as i64;
            let val = json!({
                "cpu": cpu,
                "diskread": 1034,
                "diskwrite": 1034,
                "disk": disk.saturating_add_signed(jitter),
                "id": format!("lxc/{vmid}"),
                "level": "",
                "maxcpu": maxcpu,
                "maxdisk": maxdisk,
                "maxmem": maxmem,
                "mem": mem.saturating_add_signed(jitter),
                "memhost": memhost.saturating_add_signed(jitter),
                "name": format!("ct-{vmid}"),
                "netin": 1034,
                "netout": 1034,
                "node": format!("node-{}", vmid % self.nr_of_nodes),
                "status": "running",
                "template": false,
                "type": "lxc",
                "uptime": 1234,
                "vmid": vmid,
            });
            result.push(serde_json::from_value(val).map_err(|err| {
                proxmox_client::Error::Anyhow(format_err!("error on ct {vmid}: {err}"))
            })?);
        }

        for i in 0..self.nr_of_nodes {
            let jitter = ((i as f64).sin() * bytejitter).round() as i64;
            let val = json!({
                "cpu": cpu,
                "disk": disk.saturating_add_signed(jitter),
                "id": format!("node/node-{i}"),
                "level": "",
                "maxcpu": 16.0,
                "maxdisk": maxdisk,
                "maxmem": maxmem,
                "mem": mem.saturating_add_signed(jitter),
                "node": format!("node-{i}"),
                "status": "online",
                "type": "node",
                "uptime": 1234,
            });
            result.push(serde_json::from_value(val).map_err(|err| {
                proxmox_client::Error::Anyhow(format_err!("error on node {i}: {err}"))
            })?);
        }

        for i in 0..self.nr_of_storages {
            let jitter = ((i as f64).sin() * bytejitter).round() as i64;
            let val = json!({
                "content": "images,rootdir",
                "disk": disk.saturating_add_signed(jitter),
                "id": format!("storage/node-0/storage-{i}"),
                "maxdisk": maxdisk,
                "node": format!("node-{}", i % self.nr_of_nodes),
                "plugintype": "dir",
                "status": "available",
                "storage": format!("storage-{i}"),
                "type": "storage",
            });
            result.push(serde_json::from_value(val).map_err(|err| {
                proxmox_client::Error::Anyhow(format_err!("error on storage {i}: {err}"))
            })?);
        }

        tokio::time::sleep(Duration::from_millis(self.api_delay_ms as u64)).await;

        Ok(result)
    }

    /// Retrieve metrics of the cluster.
    async fn cluster_metrics_export(
        &self,
        _history: Option<bool>,
        _local_only: Option<bool>,
        _node_list: Option<String>,
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

    async fn list_nodes(&self) -> Result<Vec<ClusterNodeIndexResponse>, proxmox_client::Error> {
        tokio::time::sleep(Duration::from_millis(self.api_delay_ms as u64)).await;
        Ok((0..self.nr_of_nodes)
            .map(|i| ClusterNodeIndexResponse {
                node: format!("pve-{i}"),
                cpu: None,
                level: None,
                maxcpu: None,
                maxmem: None,
                mem: None,
                ssl_fingerprint: None,
                status: ClusterNodeIndexResponseStatus::Online,
                uptime: None,
            })
            .collect())
    }

    async fn get_task_list(
        &self,
        node: &str,
        params: ListTasks,
    ) -> Result<Vec<ListTasksResponse>, proxmox_client::Error> {
        tokio::time::sleep(Duration::from_millis(self.api_delay_ms as u64)).await;
        let make_task = |starttime| {
            let endtime = Some(starttime + 4);

            let upid_str =
                format!("UPID:{node}:0000C530:001C9BEC:{starttime:08X}:stopall::root@pam:",);
            let upid: PveUpid = upid_str.parse().unwrap();

            ListTasksResponse {
                node: node.to_string(),
                endtime,
                pid: upid.pid as i64,
                pstart: upid.pstart as i64,
                starttime,
                status: Some("OK".to_string()),
                ty: upid.worker_type,
                user: upid.auth_id,
                upid: upid_str,
                id: upid.worker_id.unwrap_or_default(),
            }
        };

        const DEFAULT_LIMIT: u64 = 1500;
        const DEFAULT_SINCE: i64 = 0;
        // Let's fake a new task every 5 minutes
        const NEW_TASK_EVERY: i64 = 5;

        let limit = params.limit.unwrap_or(DEFAULT_LIMIT);
        let since = params.since.unwrap_or(DEFAULT_SINCE);

        let now = proxmox_time::epoch_i64();

        let number_of_tasks = (now - since) / (NEW_TASK_EVERY * 60);

        let number_of_tasks = limit.min(number_of_tasks as u64);

        Ok((0..number_of_tasks)
            .map(|i| make_task(now - i as i64 * NEW_TASK_EVERY * 60))
            .collect())
    }
}
