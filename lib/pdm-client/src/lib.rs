//! Proxmox Datacenter Manager API client.

use std::collections::HashMap;
use std::time::Duration;

use pdm_api_types::remotes::{RemoteType, TlsProbeOutcome};
use pdm_api_types::resource::{PveResource, RemoteResources, ResourceType, TopEntities};
use pdm_api_types::rrddata::{
    LxcDataPoint, NodeDataPoint, PbsDatastoreDataPoint, PbsNodeDataPoint, PveStorageDataPoint,
    QemuDataPoint,
};
use pdm_api_types::sdn::{ListVnet, ListZone};
use pdm_api_types::BasicRealmInfo;
use pve_api_types::StartQemuMigrationType;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use proxmox_client::{ApiPathBuilder, Error, HttpApiClient};
use proxmox_rrd_api_types::{RrdMode, RrdTimeframe};

use types::*;
/// For convenience we reexport all the api types the client uses.
pub mod types {
    pub use proxmox_access_control::types::{User, UserWithTokens};

    pub use pdm_api_types::remotes::Remote;
    pub use pdm_api_types::{AclListItem, Authid, ConfigurationState, RemoteUpid};

    pub use pve_api_types::{ClusterNodeIndexResponse, ClusterNodeIndexResponseStatus};

    pub use pve_api_types::{ListNetworksType, NetworkInterface, NetworkInterfaceType};

    pub use pve_api_types::ClusterResourceKind;

    pub use pve_api_types::{StorageContent, StorageInfo};

    pub use pve_api_types::{IsRunning, LxcStatus, QemuStatus};

    pub use pve_api_types::verifiers::VOLUME_ID;
    pub use pve_api_types::{
        LxcConfig, LxcConfigMp, LxcConfigNet, LxcConfigRootfs, LxcConfigUnused, PveQmIde,
        QemuConfig, QemuConfigNet, QemuConfigNetModel, QemuConfigSata, QemuConfigScsi,
        QemuConfigUnused, QemuConfigVirtio,
    };

    pub use pdm_api_types::resource::{Resource, ResourceRrdData};

    pub use pve_api_types::NodeStatus;

    pub use pdm_api_types::resource::{TopEntities, TopEntity};

    pub use pve_api_types::{
        QemuMigratePreconditions, QemuMigratePreconditionsLocalDisks,
        QemuMigratePreconditionsNotAllowedNodes,
    };

    pub use pve_api_types::ListRealm;

    pub use pve_api_types::ClusterNodeStatus;

    pub use pve_api_types::PveUpid;

    pub use pdm_api_types::sdn::{
        CreateVnetParams, CreateZoneParams, ListController, ListVnet, ListZone, SDN_ID_SCHEMA,
    };
    pub use pve_api_types::{ListControllersType, ListZonesType, SdnObjectState};

    pub use pve_api_types::ClusterResourceNetworkType;

    pub use pve_api_types::StorageStatus as PveStorageStatus;

    pub use pdm_api_types::subscription::{RemoteSubscriptionState, RemoteSubscriptions};

    pub use pve_api_types::SdnZoneIpVrf;
}

pub struct PdmClient<T: HttpApiClient>(pub T);

impl<T: HttpApiClient> std::ops::Deref for PdmClient<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: HttpApiClient> std::ops::DerefMut for PdmClient<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: HttpApiClient> PdmClient<T> {
    pub async fn list_remotes(&self) -> Result<Vec<Remote>, Error> {
        Ok(self.0.get("/api2/extjs/remotes").await?.expect_json()?.data)
    }

    pub async fn add_remote(
        &self,
        remote: &Remote,
        create_token: Option<&str>,
    ) -> Result<(), proxmox_client::Error> {
        #[derive(Serialize)]
        #[serde(rename_all = "kebab-case")]
        struct AddRemoteParams<'a> {
            #[serde(flatten)]
            remote: &'a Remote,
            #[serde(skip_serializing_if = "Option::is_none")]
            create_token: Option<&'a str>,
        }
        self.0
            .post(
                "/api2/extjs/remotes",
                &AddRemoteParams {
                    remote,
                    create_token,
                },
            )
            .await?
            .nodata()
    }

    pub async fn update_remote(
        &self,
        remote: &str,
        updater: &pdm_api_types::remotes::RemoteUpdater,
    ) -> Result<(), Error> {
        let path = format!("/api2/extjs/remotes/{remote}");
        self.0.put(&path, updater).await?.nodata()?;
        Ok(())
    }

    pub async fn delete_remote(&self, remote: &str) -> Result<(), Error> {
        let path = format!("/api2/extjs/remotes/{remote}");
        self.0.delete(&path).await?.nodata()?;
        Ok(())
    }

    pub async fn remote_version(
        &self,
        remote: &str,
    ) -> Result<pve_api_types::VersionResponse, proxmox_client::Error> {
        let path = format!("/api2/extjs/remotes/{remote}/version");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn read_user(&self, user: &str) -> Result<User, Error> {
        let path = format!("/api2/extjs/access/users/{user}");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn list_users(&self, include_api_tokens: bool) -> Result<Vec<UserWithTokens>, Error> {
        let path = ApiPathBuilder::new("/api2/extjs/access/users")
            .arg("include_tokens", include_api_tokens)
            .build();
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn list_realms(&self) -> Result<Vec<BasicRealmInfo>, Error> {
        Ok(self
            .0
            .get("/api2/extjs/access/domains")
            .await?
            .expect_json()?
            .data)
    }

    pub async fn create_user(&self, config: &User, password: Option<&str>) -> Result<(), Error> {
        #[derive(Serialize)]
        struct CreateUser<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            password: Option<&'a str>,
            #[serde(flatten)]
            config: &'a User,
        }

        let path = "/api2/extjs/access/users";
        self.0
            .post(path, &CreateUser { password, config })
            .await?
            .nodata()
    }

    pub async fn update_user(
        &self,
        userid: &str,
        updater: &proxmox_access_control::types::UserUpdater,
        password: Option<&str>,
        delete: &[pdm_api_types::DeletableUserProperty],
    ) -> Result<(), Error> {
        #[derive(Serialize)]
        struct UpdateUser<'a> {
            #[serde(flatten)]
            updater: &'a proxmox_access_control::types::UserUpdater,
            #[serde(skip_serializing_if = "Option::is_none")]
            password: Option<&'a str>,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            delete: Vec<String>,
        }

        let delete = delete.iter().map(|d| d.to_string()).collect::<Vec<_>>();

        let path = format!("/api2/extjs/access/users/{userid}");
        self.0
            .put(
                &path,
                &UpdateUser {
                    updater,
                    password,
                    delete,
                },
            )
            .await?
            .nodata()
    }

    pub async fn delete_user(&self, userid: &str) -> Result<(), Error> {
        let path = format!("/api2/extjs/access/users/{userid}");
        self.0.delete(&path).await?.nodata()
    }

    pub async fn list_user_tfa(
        &self,
        userid: &str,
    ) -> Result<Vec<proxmox_tfa::TypedTfaInfo>, Error> {
        let path = format!("/api2/extjs/access/tfa/{userid}");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn delete_tfa_entry(
        &self,
        userid: &str,
        password: Option<&str>,
        id: &str,
    ) -> Result<(), proxmox_client::Error> {
        let path = ApiPathBuilder::new(format!("/api2/extjs/access/tfa/{userid}/{id}"))
            .maybe_arg("password", &password)
            .build();
        self.0.delete(&path).await?.nodata()
    }

    pub async fn add_recovery_keys(
        &self,
        userid: &str,
        password: Option<&str>,
        description: &str,
    ) -> Result<Vec<String>, Error> {
        let path = format!("/api2/extjs/access/tfa/{userid}");

        let result: proxmox_tfa::TfaUpdateInfo = self
            .0
            .post(
                &path,
                &AddTfaEntry {
                    ty: proxmox_tfa::TfaType::Recovery,
                    description: Some(description.to_string()),
                    password: password.map(str::to_owned),
                    ..AddTfaEntry::empty()
                },
            )
            .await?
            .expect_json()?
            .data;

        if result.recovery.is_empty() {
            return Err(Error::BadApi(
                "api returned empty list of recovery keys".to_string(),
                None,
            ));
        }

        Ok(result.recovery)
    }

    pub async fn add_webauthn(
        &self,
        userid: &str,
        password: Option<&str>,
        description: &str,
    ) -> Result<String, Error> {
        let path = format!("/api2/extjs/access/tfa/{userid}");

        let result: proxmox_tfa::TfaUpdateInfo = self
            .0
            .post(
                &path,
                &AddTfaEntry {
                    ty: proxmox_tfa::TfaType::Webauthn,
                    description: Some(description.to_string()),
                    password: password.map(str::to_owned),
                    ..AddTfaEntry::empty()
                },
            )
            .await?
            .expect_json()?
            .data;

        result.challenge.ok_or_else(|| {
            Error::BadApi(
                "api returned no challenge to confirm webauthn entry".to_string(),
                None,
            )
        })
    }

    pub async fn add_webauthn_finish(
        &self,
        userid: &str,
        password: Option<&str>,
        challenge: &str,
        response: &str,
    ) -> Result<String, Error> {
        let path = format!("/api2/extjs/access/tfa/{userid}");

        let result: proxmox_tfa::TfaUpdateInfo = self
            .0
            .post(
                &path,
                &AddTfaEntry {
                    ty: proxmox_tfa::TfaType::Webauthn,
                    challenge: Some(challenge.to_string()),
                    value: Some(response.to_string()),
                    password: password.map(str::to_owned),
                    ..AddTfaEntry::empty()
                },
            )
            .await?
            .expect_json()?
            .data;

        result
            .id
            .ok_or_else(|| Error::BadApi("api returned no webauthn entry id".to_string(), None))
    }

    /// Trigger metric collection for a single remote or for all remotes, if no remote is provided.
    pub async fn trigger_metric_collection(
        &self,
        remote: Option<&str>,
    ) -> Result<(), proxmox_client::Error> {
        let path = "/api2/extjs/metric-collection/trigger";

        #[derive(Serialize)]
        struct TriggerParams<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            remote: Option<&'a str>,
        }

        self.0
            .post(path, &TriggerParams { remote })
            .await?
            .nodata()?;

        Ok(())
    }

    /// Get global metric collection status.
    pub async fn get_metric_collection_status(
        &self,
    ) -> Result<Vec<pdm_api_types::MetricCollectionStatus>, Error> {
        let path = "/api2/extjs/metric-collection/status";
        Ok(self.0.get(path).await?.expect_json()?.data)
    }

    /// Get PDM node RRD data.
    pub async fn get_pdm_node_rrddata(
        &self,
        mode: RrdMode,
        timeframe: RrdTimeframe,
    ) -> Result<pdm_api_types::rrddata::PdmNodeDatapoint, Error> {
        let path = ApiPathBuilder::new("/api2/extjs/nodes/localhost/rrddata")
            .arg("cf", mode)
            .arg("timeframe", timeframe)
            .build();
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    /// Get per-remote RRD data.
    pub async fn get_per_remote_rrddata(
        &self,
        remote: &str,
        mode: RrdMode,
        timeframe: RrdTimeframe,
    ) -> Result<pdm_api_types::rrddata::RemoteDatapoint, Error> {
        let path = ApiPathBuilder::new(format!("/api2/extjs/remotes/{remote}/rrddata"))
            .arg("cf", mode)
            .arg("timeframe", timeframe)
            .build();
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_list_nodes(
        &self,
        remote: &str,
    ) -> Result<Vec<pve_api_types::ClusterNodeIndexResponse>, Error> {
        let path = format!("/api2/extjs/pve/remotes/{remote}/nodes");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_get_firewall_status(
        &self,
    ) -> Result<Vec<pdm_api_types::firewall::RemoteFirewallStatus>, Error> {
        let path = "/api2/extjs/pve/firewall/status";
        Ok(self.0.get(path).await?.expect_json()?.data)
    }

    pub async fn pve_cluster_firewall_status(
        &self,
        remote: &str,
    ) -> Result<pdm_api_types::firewall::RemoteFirewallStatus, Error> {
        let path = format!("/api2/extjs/pve/remotes/{remote}/firewall/status");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_node_firewall_status(
        &self,
        remote: &str,
        node: &str,
    ) -> Result<pdm_api_types::firewall::NodeFirewallStatus, Error> {
        let path = format!("/api2/extjs/pve/remotes/{remote}/nodes/{node}/firewall/status");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_get_cluster_firewall_options(
        &self,
        remote: &str,
    ) -> Result<pve_api_types::ClusterFirewallOptions, Error> {
        let path = format!("/api2/extjs/pve/remotes/{remote}/firewall/options");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_get_node_firewall_options(
        &self,
        remote: &str,
        node: &str,
    ) -> Result<pve_api_types::NodeFirewallOptions, Error> {
        let path = format!("/api2/extjs/pve/remotes/{remote}/nodes/{node}/firewall/options");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_get_lxc_firewall_options(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
    ) -> Result<pve_api_types::GuestFirewallOptions, Error> {
        let path = ApiPathBuilder::new(format!(
            "/api2/extjs/pve/remotes/{remote}/lxc/{vmid}/firewall/options"
        ))
        .maybe_arg("node", &node)
        .build();
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_get_qemu_firewall_options(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
    ) -> Result<pve_api_types::GuestFirewallOptions, Error> {
        let path = ApiPathBuilder::new(format!(
            "/api2/extjs/pve/remotes/{remote}/qemu/{vmid}/firewall/options"
        ))
        .maybe_arg("node", &node)
        .build();
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_get_cluster_firewall_rules(
        &self,
        remote: &str,
    ) -> Result<Vec<pve_api_types::ListFirewallRules>, Error> {
        let path = format!("/api2/extjs/pve/remotes/{remote}/firewall/rules");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_get_node_firewall_rules(
        &self,
        remote: &str,
        node: &str,
    ) -> Result<Vec<pve_api_types::ListFirewallRules>, Error> {
        let path = format!("/api2/extjs/pve/remotes/{remote}/nodes/{node}/firewall/rules");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_get_lxc_firewall_rules(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
    ) -> Result<Vec<pve_api_types::ListFirewallRules>, Error> {
        let path = ApiPathBuilder::new(format!(
            "/api2/extjs/pve/remotes/{remote}/lxc/{vmid}/firewall/rules"
        ))
        .maybe_arg("node", &node)
        .build();
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_get_qemu_firewall_rules(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
    ) -> Result<Vec<pve_api_types::ListFirewallRules>, Error> {
        let path = ApiPathBuilder::new(format!(
            "/api2/extjs/pve/remotes/{remote}/qemu/{vmid}/firewall/rules"
        ))
        .maybe_arg("node", &node)
        .build();
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_set_cluster_firewall_options(
        &self,
        remote: &str,
        update: pve_api_types::UpdateClusterFirewallOptions,
    ) -> Result<(), Error> {
        let path = format!("/api2/extjs/pve/remotes/{remote}/firewall/options");
        self.0.put(&path, &update).await?.nodata()
    }

    pub async fn pve_set_node_firewall_options(
        &self,
        remote: &str,
        node: &str,
        update: pve_api_types::UpdateNodeFirewallOptions,
    ) -> Result<(), Error> {
        let path = format!("/api2/extjs/pve/remotes/{remote}/nodes/{node}/firewall/options");
        self.0.put(&path, &update).await?.nodata()
    }

    pub async fn pve_node_rrddata(
        &self,
        remote: &str,
        node: &str,
        mode: RrdMode,
        timeframe: RrdTimeframe,
    ) -> Result<Vec<NodeDataPoint>, Error> {
        let path = format!(
            "/api2/extjs/pve/remotes/{remote}/nodes/{node}/rrddata?cf={mode}&timeframe={timeframe}"
        );
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_cluster_resources(
        &self,
        remote: &str,
        kind: Option<pve_api_types::ClusterResourceKind>,
    ) -> Result<Vec<PveResource>, Error> {
        let query = ApiPathBuilder::new(format!("/api2/extjs/pve/remotes/{remote}/resources"))
            .maybe_arg("kind", &kind)
            .build();
        Ok(self.0.get(&query).await?.expect_json()?.data)
    }

    pub async fn pve_cluster_status(
        &self,
        remote: &str,
        target_endpoint: Option<&str>,
    ) -> Result<Vec<ClusterNodeStatus>, Error> {
        let query = ApiPathBuilder::new(format!("/api2/extjs/pve/remotes/{remote}/cluster-status"))
            .maybe_arg("target-endpoint", &target_endpoint)
            .build();
        Ok(self.0.get(&query).await?.expect_json()?.data)
    }

    pub async fn pve_list_qemu(
        &self,
        remote: &str,
        node: Option<&str>,
    ) -> Result<Vec<pve_api_types::VmEntry>, Error> {
        let path = ApiPathBuilder::new(format!("/api2/extjs/pve/remotes/{remote}/qemu"))
            .maybe_arg("node", &node)
            .build();
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_list_lxc(
        &self,
        remote: &str,
        node: Option<&str>,
    ) -> Result<Vec<pve_api_types::VmEntry>, Error> {
        let path = ApiPathBuilder::new(format!("/api2/extjs/pve/remotes/{remote}/lxc"))
            .maybe_arg("node", &node)
            .build();
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_qemu_config(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
        state: ConfigurationState,
        snapshot: Option<&str>,
    ) -> Result<pve_api_types::QemuConfig, Error> {
        let path = ApiPathBuilder::new(format!(
            "/api2/extjs/pve/remotes/{remote}/qemu/{vmid}/config"
        ))
        .arg("state", state)
        .maybe_arg("node", &node)
        .maybe_arg("snapshot", &snapshot)
        .build();
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_qemu_status(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
    ) -> Result<pve_api_types::QemuStatus, Error> {
        let path = ApiPathBuilder::new(format!(
            "/api2/extjs/pve/remotes/{remote}/qemu/{vmid}/status"
        ))
        .maybe_arg("node", &node)
        .build();
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_lxc_status(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
    ) -> Result<pve_api_types::LxcStatus, Error> {
        let path = ApiPathBuilder::new(format!(
            "/api2/extjs/pve/remotes/{remote}/lxc/{vmid}/status"
        ))
        .maybe_arg("node", &node)
        .build();
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    async fn pve_change_guest_status(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
        vmtype: &str,
        action: &str,
    ) -> Result<RemoteUpid, Error> {
        let path = format!("/api2/extjs/pve/remotes/{remote}/{vmtype}/{vmid}/{action}");
        let mut request = json!({});
        if let Some(node) = node {
            request["node"] = node.into();
        }
        Ok(self.0.post(&path, &request).await?.expect_json()?.data)
    }

    pub async fn pve_qemu_start(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
    ) -> Result<RemoteUpid, Error> {
        self.pve_change_guest_status(remote, node, vmid, "qemu", "start")
            .await
    }

    pub async fn pve_qemu_shutdown(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
    ) -> Result<RemoteUpid, Error> {
        self.pve_change_guest_status(remote, node, vmid, "qemu", "shutdown")
            .await
    }

    pub async fn pve_qemu_stop(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
    ) -> Result<RemoteUpid, Error> {
        self.pve_change_guest_status(remote, node, vmid, "qemu", "stop")
            .await
    }

    pub async fn pve_qemu_migrate(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
        target: String,
        params: MigrateQemu,
    ) -> Result<RemoteUpid, Error> {
        let path = format!("/api2/extjs/pve/remotes/{remote}/qemu/{vmid}/migrate");
        let mut request = serde_json::to_value(&params).expect("failed to build json string");
        request["target"] = target.into();
        if let Some(node) = node {
            request["node"] = node.into();
        }
        Ok(self.0.post(&path, &request).await?.expect_json()?.data)
    }

    pub async fn pve_qemu_remote_migrate(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
        target: String,
        target_endpoint: Option<&str>,
        params: RemoteMigrateQemu,
    ) -> Result<RemoteUpid, Error> {
        let path = format!("/api2/extjs/pve/remotes/{remote}/qemu/{vmid}/remote-migrate");
        let mut request = serde_json::to_value(&params).expect("failed to build json string");
        request["target"] = target.into();
        if let Some(node) = node {
            request["node"] = node.into();
        }
        if let Some(target_endpoint) = target_endpoint {
            request["target-endpoint"] = target_endpoint.into();
        }
        Ok(self.0.post(&path, &request).await?.expect_json()?.data)
    }

    pub async fn pve_qemu_rrddata(
        &self,
        remote: &str,
        vmid: u32,
        mode: RrdMode,
        timeframe: RrdTimeframe,
    ) -> Result<Vec<QemuDataPoint>, Error> {
        let path = format!(
            "/api2/extjs/pve/remotes/{remote}/qemu/{vmid}/rrddata?cf={mode}&timeframe={timeframe}"
        );
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_lxc_config(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
        state: ConfigurationState,
        snapshot: Option<&str>,
    ) -> Result<pve_api_types::LxcConfig, Error> {
        let path = ApiPathBuilder::new(format!(
            "/api2/extjs/pve/remotes/{remote}/lxc/{vmid}/config"
        ))
        .maybe_arg("node", &node)
        .arg("state", state)
        .maybe_arg("snapshot", &snapshot)
        .build();
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_lxc_start(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
    ) -> Result<RemoteUpid, Error> {
        self.pve_change_guest_status(remote, node, vmid, "lxc", "start")
            .await
    }

    pub async fn pve_lxc_shutdown(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
    ) -> Result<RemoteUpid, Error> {
        self.pve_change_guest_status(remote, node, vmid, "lxc", "shutdown")
            .await
    }

    pub async fn pve_lxc_stop(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
    ) -> Result<RemoteUpid, Error> {
        self.pve_change_guest_status(remote, node, vmid, "lxc", "stop")
            .await
    }

    pub async fn pve_lxc_migrate(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
        target: String,
        params: MigrateLxc,
    ) -> Result<RemoteUpid, Error> {
        let path = format!("/api2/extjs/pve/remotes/{remote}/lxc/{vmid}/migrate");
        let mut request = serde_json::to_value(&params).expect("failed to build json string");
        request["target"] = target.into();
        if let Some(node) = node {
            request["node"] = node.into();
        }
        Ok(self.0.post(&path, &request).await?.expect_json()?.data)
    }

    pub async fn pve_lxc_remote_migrate(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
        target: String,
        target_endpoint: Option<&str>,
        params: RemoteMigrateLxc,
    ) -> Result<RemoteUpid, Error> {
        let path = format!("/api2/extjs/pve/remotes/{remote}/lxc/{vmid}/remote-migrate");
        let mut request = serde_json::to_value(&params).expect("failed to build json string");
        request["target"] = target.into();
        if let Some(node) = node {
            request["node"] = node.into();
        }
        if let Some(target_endpoint) = target_endpoint {
            request["target-endpoint"] = target_endpoint.into();
        }
        Ok(self.0.post(&path, &request).await?.expect_json()?.data)
    }

    pub async fn pve_lxc_rrddata(
        &self,
        remote: &str,
        vmid: u32,
        mode: RrdMode,
        timeframe: RrdTimeframe,
    ) -> Result<Vec<LxcDataPoint>, Error> {
        let path = format!(
            "/api2/extjs/pve/remotes/{remote}/lxc/{vmid}/rrddata?cf={mode}&timeframe={timeframe}"
        );
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_list_tasks(
        &self,
        remote: &str,
        node: Option<&str>,
    ) -> Result<Vec<pve_api_types::ListTasksResponse>, Error> {
        let query = ApiPathBuilder::new(format!("/api2/extjs/pve/remotes/{remote}/tasks"))
            .maybe_arg("node", &node)
            .build();
        Ok(self.0.get(&query).await?.expect_json()?.data)
    }

    pub async fn pve_stop_task(&self, remote: &str, upid: &str) -> Result<(), Error> {
        let path = format!("/api2/extjs/pve/remotes/{remote}/tasks/{upid}");
        #[allow(clippy::unit_arg)]
        Ok(self.0.delete(&path).await?.expect_json()?.data)
    }

    pub async fn pve_task_status(
        &self,
        upid: &RemoteUpid,
    ) -> Result<pve_api_types::TaskStatus, Error> {
        let remote = upid.remote();
        let upid = upid.to_string();
        let path = format!("/api2/extjs/pve/remotes/{remote}/tasks/{upid}/status");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_wait_for_task(
        &self,
        upid: &RemoteUpid,
    ) -> Result<pve_api_types::TaskStatus, Error> {
        let remote = upid.remote();
        let upid = upid.to_string();
        let path = format!("/api2/extjs/pve/remotes/{remote}/tasks/{upid}/status?wait=1");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn read_acl(
        &self,
        path: Option<&str>,
        exact: bool,
    ) -> Result<(Vec<AclListItem>, Option<ConfigDigest>), Error> {
        let query = ApiPathBuilder::new("/api2/extjs/access/acl")
            .arg("exact", exact as u8)
            .maybe_arg("path", &path)
            .build();
        let mut res = self.0.get(&query).await?.expect_json()?;
        Ok((res.data, res.attribs.remove("digest").map(ConfigDigest)))
    }

    pub async fn update_acl(
        &self,
        recipient: AclRecipient<'_>,
        path: &str,
        role: &str,
        propagate: bool,
        digest: Option<ConfigDigest>,
    ) -> Result<(), Error> {
        #[derive(Serialize)]
        struct UpdateAclArgs<'a> {
            path: &'a str,
            role: &'a str,
            propagate: bool,
            #[serde(flatten)]
            recipient: AclRecipient<'a>,
            #[serde(skip_serializing_if = "Option::is_none")]
            digest: Option<ConfigDigest>,
        }

        let api_path = "/api2/extjs/access/acl";
        self.0
            .put(
                api_path,
                &UpdateAclArgs {
                    path,
                    role,
                    propagate,
                    recipient,
                    digest,
                },
            )
            .await?
            .nodata()
    }

    pub async fn delete_acl(
        &self,
        recipient: AclRecipient<'_>,
        path: &str,
        role: &str,
        digest: Option<ConfigDigest>,
    ) -> Result<(), Error> {
        #[derive(Serialize)]
        struct UpdateAclArgs<'a> {
            path: &'a str,
            role: &'a str,
            #[serde(flatten)]
            recipient: AclRecipient<'a>,
            delete: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            digest: Option<ConfigDigest>,
        }

        let api_path = "/api2/extjs/access/acl";
        self.0
            .put(
                api_path,
                &UpdateAclArgs {
                    path,
                    role,
                    recipient,
                    delete: true,
                    digest,
                },
            )
            .await?
            .nodata()
    }

    pub async fn pbs_list_datastores(
        &self,
        remote: &str,
    ) -> Result<Vec<pbs_api_types::DataStoreConfig>, Error> {
        let path = format!("/api2/extjs/pbs/remotes/{remote}/datastore");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pbs_list_snapshots(
        &self,
        remote: &str,
        store: &str,
        namespace: Option<&str>,
    ) -> Result<Vec<pbs_api_types::SnapshotListItem>, Error> {
        let path = ApiPathBuilder::new(format!(
            "/api2/extjs/pbs/remotes/{remote}/datastore/{store}/snapshots"
        ))
        .maybe_arg("ns", &namespace)
        .build();
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pbs_node_rrddata(
        &self,
        remote: &str,
        mode: RrdMode,
        timeframe: RrdTimeframe,
    ) -> Result<Vec<PbsNodeDataPoint>, Error> {
        let path =
            format!("/api2/extjs/pbs/remotes/{remote}/rrddata?cf={mode}&timeframe={timeframe}");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pbs_node_status(&self, remote: &str) -> Result<pbs_api_types::NodeStatus, Error> {
        let path = format!("/api2/extjs/pbs/remotes/{remote}/status");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pbs_datastore_rrddata(
        &self,
        remote: &str,
        store: &str,
        mode: RrdMode,
        timeframe: RrdTimeframe,
    ) -> Result<Vec<PbsDatastoreDataPoint>, Error> {
        let path = ApiPathBuilder::new(format!(
            "/api2/extjs/pbs/remotes/{remote}/datastore/{store}/rrddata"
        ))
        .arg("cf", mode)
        .arg("timeframe", timeframe)
        .build();
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pbs_list_tasks(
        &self,
        remote: &str,
    ) -> Result<Vec<pbs_api_types::TaskListItem>, Error> {
        let query = format!("/api2/extjs/pbs/remotes/{remote}/tasks");
        Ok(self.0.get(&query).await?.expect_json()?.data)
    }

    pub async fn pbs_task_status(
        &self,
        upid: &RemoteUpid,
    ) -> Result<pdm_api_types::pbs::TaskStatus, Error> {
        let remote = upid.remote();
        let upid = upid.to_string();
        let path = format!("/api2/extjs/pbs/remotes/{remote}/tasks/{upid}/status");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn resources(
        &self,
        max_age: Option<u64>,
        view: Option<&str>,
    ) -> Result<Vec<RemoteResources>, Error> {
        let path = ApiPathBuilder::new("/api2/extjs/resources/list")
            .maybe_arg("max-age", &max_age)
            .maybe_arg("view", &view)
            .build();
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn resources_by_type(
        &self,
        max_age: Option<u64>,
        resource_type: ResourceType,
        view: Option<&str>,
    ) -> Result<Vec<RemoteResources>, Error> {
        let path = ApiPathBuilder::new("/api2/extjs/resources/list")
            .maybe_arg("max-age", &max_age)
            .arg("resource-type", resource_type)
            .maybe_arg("view", &view)
            .build();

        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    /// Get the subscription status.
    pub async fn get_subscription_status(
        &self,
        max_age: Option<u64>,
        verbose: Option<bool>,
        view: Option<&str>,
    ) -> Result<Vec<RemoteSubscriptions>, Error> {
        let path = ApiPathBuilder::new("/api2/extjs/resources/subscription")
            .maybe_arg("max-age", &max_age)
            .maybe_arg("verbose", &verbose)
            .maybe_arg("view", &view)
            .build();

        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_list_networks(
        &self,
        remote: &str,
        node: &str,
        interface_type: Option<ListNetworksType>,
    ) -> Result<Vec<NetworkInterface>, Error> {
        let path = ApiPathBuilder::new(format!(
            "/api2/extjs/pve/remotes/{remote}/nodes/{node}/network"
        ))
        .maybe_arg("interface-type", &interface_type)
        .build();
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_list_storages(
        &self,
        remote: &str,
        node: &str,
        content: Option<Vec<StorageContent>>,
        enabled: Option<bool>,
        format: Option<bool>,
        storage: Option<String>,
        target: Option<String>,
    ) -> Result<Vec<StorageInfo>, Error> {
        let mut builder = ApiPathBuilder::new(format!(
            "/api2/extjs/pve/remotes/{remote}/nodes/{node}/storage"
        ))
        .maybe_arg("enabled", &enabled)
        .maybe_arg("format", &format)
        .maybe_arg("storage", &storage)
        .maybe_arg("target", &target);
        if let Some(content) = content {
            for ty in content {
                builder = builder.arg("content", ty);
            }
        }
        let path = builder.build();

        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_storage_status(
        &self,
        remote: &str,
        node: &str,
        storage: &str,
    ) -> Result<PveStorageStatus, Error> {
        let path =
            format!("/api2/extjs/pve/remotes/{remote}/nodes/{node}/storage/{storage}/status");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_storage_rrddata(
        &self,
        remote: &str,
        node: &str,
        storage: &str,
        mode: RrdMode,
        timeframe: RrdTimeframe,
    ) -> Result<Vec<PveStorageDataPoint>, Error> {
        let path = format!(
            "/api2/extjs/pve/remotes/{remote}/nodes/{node}/storage/{storage}/rrddata?cf={mode}&timeframe={timeframe}"
        );
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn get_top_entities(&self, view: Option<&str>) -> Result<TopEntities, Error> {
        let builder = ApiPathBuilder::new("/api2/extjs/resources/top-entities".to_string())
            .maybe_arg("view", &view);

        let path = builder.build();

        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_node_status(&self, remote: &str, node: &str) -> Result<NodeStatus, Error> {
        let path = format!("/api2/extjs/pve/remotes/{remote}/nodes/{node}/status");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_qemu_migrate_preconditions(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
        target: Option<String>,
    ) -> Result<QemuMigratePreconditions, Error> {
        let path = ApiPathBuilder::new(format!(
            "/api2/extjs/pve/remotes/{remote}/qemu/{vmid}/migrate"
        ))
        .maybe_arg("node", &node)
        .maybe_arg("target", &target)
        .build();
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    /// uses /pve/probe-tls to probe the tls connection to the given host
    pub async fn pve_probe_tls(
        &self,
        hostname: &str,
        fingerprint: Option<&str>,
    ) -> Result<TlsProbeOutcome, Error> {
        self.probe_tls(hostname, fingerprint, RemoteType::Pve).await
    }

    /// Uses /pve/scan to scan the remote cluster for node/fingerprint information
    pub async fn pve_scan_remote(
        &self,
        hostname: &str,
        fingerprint: Option<&str>,
        authid: &str,
        token: &str,
    ) -> Result<Remote, Error> {
        self.scan_remote(hostname, fingerprint, authid, token, RemoteType::Pve)
            .await
    }

    pub async fn pve_sdn_list_controllers(
        &self,
        pending: impl Into<Option<bool>>,
        running: impl Into<Option<bool>>,
        ty: impl Into<Option<ListControllersType>>,
    ) -> Result<Vec<ListController>, Error> {
        let path = ApiPathBuilder::new("/api2/extjs/sdn/controllers")
            .maybe_arg("pending", &pending.into())
            .maybe_arg("running", &running.into())
            .maybe_arg("ty", &ty.into())
            .build();

        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_sdn_list_zones(
        &self,
        pending: impl Into<Option<bool>>,
        running: impl Into<Option<bool>>,
        ty: impl Into<Option<ListZonesType>>,
    ) -> Result<Vec<ListZone>, Error> {
        let path = ApiPathBuilder::new("/api2/extjs/sdn/zones")
            .maybe_arg("pending", &pending.into())
            .maybe_arg("running", &running.into())
            .maybe_arg("ty", &ty.into())
            .build();

        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_sdn_list_vnets(
        &self,
        pending: impl Into<Option<bool>>,
        running: impl Into<Option<bool>>,
    ) -> Result<Vec<ListVnet>, Error> {
        let path = ApiPathBuilder::new("/api2/extjs/sdn/vnets")
            .maybe_arg("pending", &pending.into())
            .maybe_arg("running", &running.into())
            .build();

        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_sdn_create_zone(&self, params: CreateZoneParams) -> Result<String, Error> {
        let path = "/api2/extjs/sdn/zones";

        Ok(self.0.post(path, &params).await?.expect_json()?.data)
    }

    pub async fn pve_sdn_create_vnet(&self, params: CreateVnetParams) -> Result<String, Error> {
        let path = "/api2/extjs/sdn/vnets";

        Ok(self.0.post(path, &params).await?.expect_json()?.data)
    }

    pub async fn pve_sdn_zone_get_ip_vrf(
        &self,
        remote: &str,
        node: &str,
        zone: &str,
    ) -> Result<Vec<SdnZoneIpVrf>, Error> {
        let path = format!("/api2/extjs/pve/remotes/{remote}/nodes/{node}/sdn/zones/{zone}/ip-vrf");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    /// uses /pbs/probe-tls to probe the tls connection to the given host
    pub async fn pbs_probe_tls(
        &self,
        hostname: &str,
        fingerprint: Option<&str>,
    ) -> Result<TlsProbeOutcome, Error> {
        self.probe_tls(hostname, fingerprint, RemoteType::Pbs).await
    }

    /// uses /{remote-type}/probe-tls to probe the tls connection to the given host
    async fn probe_tls(
        &self,
        hostname: &str,
        fingerprint: Option<&str>,
        remote_type: RemoteType,
    ) -> Result<TlsProbeOutcome, Error> {
        let path = format!("/api2/extjs/{remote_type}/probe-tls");
        let mut params = json!({
            "hostname": hostname,
        });
        if let Some(fp) = fingerprint {
            params["fingerprint"] = fp.into();
        }
        Ok(self.0.post(&path, &params).await?.expect_json()?.data)
    }

    /// Uses /pbs/scan to scan the remote cluster for node/fingerprint information
    pub async fn pbs_scan_remote(
        &self,
        hostname: &str,
        fingerprint: Option<&str>,
        authid: &str,
        token: &str,
    ) -> Result<Remote, Error> {
        self.scan_remote(hostname, fingerprint, authid, token, RemoteType::Pbs)
            .await
    }

    /// Uses /{remote-type}/scan to scan the remote for node/fingerprint information
    pub async fn scan_remote(
        &self,
        hostname: &str,
        fingerprint: Option<&str>,
        authid: &str,
        token: &str,
        remote_type: RemoteType,
    ) -> Result<Remote, Error> {
        let path = format!("/api2/extjs/{remote_type}/scan");
        let mut params = json!({
            "hostname": hostname,
            "authid": authid,
            "token": token,
        });
        if let Some(fp) = fingerprint {
            params["fingerprint"] = fp.into();
        }
        Ok(self.0.post(&path, &params).await?.expect_json()?.data)
    }

    /// Get remote update summary.
    pub async fn remote_update_summary(
        &self,
    ) -> Result<pdm_api_types::remote_updates::UpdateSummary, Error> {
        Ok(self
            .0
            .get("/api2/extjs/remote-updates/summary")
            .await?
            .expect_json()?
            .data)
    }

    /// Refresh remote update summary.
    pub async fn refresh_remote_update_summary(&self) -> Result<pdm_api_types::UPID, Error> {
        Ok(self
            .0
            .post_without_body("/api2/extjs/remote-updates/refresh")
            .await?
            .expect_json()?
            .data)
    }

    /// Get remote update summary.
    pub async fn generate_system_report(&self) -> Result<String, Error> {
        Ok(self
            .0
            .get("/api2/extjs/nodes/localhost/report")
            .await?
            .expect_json()?
            .data)
    }
}

/// Builder for migration parameters.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct MigrateQemu {
    #[serde(skip_serializing_if = "Option::is_none")]
    bwlimit: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    force: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    migration_network: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    migration_type: Option<StartQemuMigrationType>,

    #[serde(skip_serializing_if = "Option::is_none")]
    online: Option<bool>,

    #[serde(rename = "target-storage", serialize_with = "stringify_target_mapping")]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    target_storage: HashMap<String, String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    with_local_disks: Option<bool>,
}

impl MigrateQemu {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bwlimit(mut self, limit: u64) -> Self {
        self.bwlimit = Some(limit);
        self
    }

    pub fn force(mut self, force: bool) -> Self {
        self.force = Some(force);
        self
    }

    pub fn map_storage<S, T>(mut self, from: S, to: T) -> Self
    where
        S: Into<String>,
        T: Into<String>,
    {
        self.target_storage.insert(from.into(), to.into());
        self
    }

    pub fn online(mut self, online: bool) -> Self {
        self.online = Some(online);
        self
    }

    pub fn with_local_disks(mut self, with_local_disks: bool) -> Self {
        self.with_local_disks = Some(with_local_disks);
        self
    }

    pub fn migration_network(mut self, migration_network: String) -> Self {
        self.migration_network = Some(migration_network);
        self
    }

    pub fn migration_type(mut self, migration_type: StartQemuMigrationType) -> Self {
        self.migration_type = Some(migration_type);
        self
    }
}

/// Builder for migration parameters.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct MigrateLxc {
    #[serde(skip_serializing_if = "Option::is_none")]
    bwlimit: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    online: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    restart: Option<bool>,

    #[serde(rename = "target-storage", serialize_with = "stringify_target_mapping")]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    target_storage: HashMap<String, String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<i64>,
}

impl MigrateLxc {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bwlimit(mut self, limit: u64) -> Self {
        self.bwlimit = Some(limit);
        self
    }

    pub fn online(mut self, online: bool) -> Self {
        self.online = Some(online);
        self
    }

    pub fn restart(mut self, restart: bool, timeout: Option<Duration>) -> Self {
        self.restart = Some(restart);
        self.timeout = timeout.map(|t| t.as_secs() as i64);
        self
    }

    pub fn map_storage<S, T>(mut self, from: S, to: T) -> Self
    where
        S: Into<String>,
        T: Into<String>,
    {
        self.target_storage.insert(from.into(), to.into());
        self
    }

    pub fn timeout(mut self, timeout: i64) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

/// Builder for remote migration parameters - common parameters.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "kebab-case")]
struct RemoteMigrateCommon {
    #[serde(skip_serializing_if = "Option::is_none")]
    target_vmid: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    delete: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    online: Option<bool>,

    #[serde(rename = "target-storage", serialize_with = "stringify_target_mapping")]
    target_storages: HashMap<String, String>,

    #[serde(rename = "target-bridge", serialize_with = "stringify_target_mapping")]
    target_bridges: HashMap<String, String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    bwlimit: Option<u64>,
}

macro_rules! remote_migrate_common_methods {
    () => {
        pub fn target_vmid(mut self, vmid: u32) -> Self {
            self.common.target_vmid = Some(vmid);
            self
        }

        pub fn delete_source(mut self, delete: bool) -> Self {
            self.common.delete = Some(delete);
            self
        }

        pub fn online(mut self, online: bool) -> Self {
            self.common.online = Some(online);
            self
        }

        pub fn bwlimit(mut self, limit: u64) -> Self {
            self.common.bwlimit = Some(limit);
            self
        }

        pub fn map_storage<S, T>(mut self, from: S, to: T) -> Self
        where
            S: Into<String>,
            T: Into<String>,
        {
            self.common.target_storages.insert(from.into(), to.into());
            self
        }

        pub fn map_bridge<S, T>(mut self, from: S, to: T) -> Self
        where
            S: Into<String>,
            T: Into<String>,
        {
            self.common.target_bridges.insert(from.into(), to.into());
            self
        }
    };
}

/// Builder for remote migration parameters.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RemoteMigrateQemu {
    #[serde(flatten)]
    common: RemoteMigrateCommon,
}

impl RemoteMigrateQemu {
    remote_migrate_common_methods!();

    pub fn new() -> Self {
        Self::default()
    }
}

/// Builder for remote migration parameters.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RemoteMigrateLxc {
    #[serde(flatten)]
    common: RemoteMigrateCommon,

    #[serde(skip_serializing_if = "Option::is_none")]
    restart: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<i64>,
}

impl RemoteMigrateLxc {
    remote_migrate_common_methods!();

    pub fn new() -> Self {
        Self::default()
    }

    pub fn restart(mut self, restart: bool, timeout: Option<Duration>) -> Self {
        self.restart = Some(restart);
        self.timeout = timeout.map(|t| t.as_secs() as i64);
        self
    }
}

fn stringify_target_mapping<S>(
    mapping: &HashMap<String, String>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if mapping.is_empty() {
        return serializer.serialize_none();
    }

    let mut list = Vec::with_capacity(mapping.len());

    if mapping.len() == 1 {
        let (key, value) = mapping.iter().next().unwrap();

        if key == "*" && value == "*" {
            // special case 1: '* = *' => identity mapping
            list.push("1".to_string());
        } else if key == "*" {
            // special case 2: '* = <something>' => single value of <something>
            list.push(value.clone());
        } else {
            list.push(format!("{key}:{value}"));
        }
    } else {
        for (from, to) in mapping.iter() {
            list.push(format!("{from}:{to}"));
        }
    }

    list.serialize(serializer)
}

#[derive(Serialize)]
struct AddTfaEntry {
    #[serde(rename = "type")]
    ty: proxmox_tfa::TfaType,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    totp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    challenge: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String>,
}

impl AddTfaEntry {
    const fn empty() -> Self {
        Self {
            ty: proxmox_tfa::TfaType::Recovery,
            description: None,
            totp: None,
            value: None,
            challenge: None,
            password: None,
        }
    }
}

/// ACL entries are either for a user or for a group.
#[derive(Clone, Serialize)]
pub enum AclRecipient<'a> {
    #[serde(rename = "auth-id")]
    Authid(&'a Authid),

    #[serde(rename = "group")]
    Group(&'a str),
}

/// Some calls return an optional configuration digest. This can be passed back to the API as-is on
/// update calls to avoid modifying things based on outdated data.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ConfigDigest(Value);

/// Digests are usually a string of a hash, but it's best to treat them as arbitrary blobs and pass
/// them back uninterpreted, therefore [`ConfigDigest`] can be converted to and back from a
/// [`serde_json::Value`].
impl From<Value> for ConfigDigest {
    fn from(value: Value) -> Self {
        Self(value)
    }
}
///
/// Digests are usually a string of a hash, but it's best to treat them as arbitrary blobs and pass
/// them back uninterpreted, therefore [`ConfigDigest`] can be converted to and back from a
/// [`serde_json::Value`].
impl From<ConfigDigest> for Value {
    fn from(value: ConfigDigest) -> Self {
        value.0
    }
}

/// From the command line we always get a `String`, therefore we allow building a [`ConfigDigest`]
/// from a `String. Note that we do not implement `FromStr` as this is not a "parsed" value,
/// instead, this should clarify that we do not want the digest to be interpreted.
impl From<String> for ConfigDigest {
    fn from(value: String) -> Self {
        Self(value.into())
    }
}
