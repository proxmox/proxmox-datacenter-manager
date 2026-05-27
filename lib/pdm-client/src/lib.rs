//! Proxmox Datacenter Manager API client.

use std::collections::HashMap;
use std::time::Duration;

use pdm_api_types::auto_installer::{
    AnswerToken, AnswerTokenCreateResult, AnswerTokenUpdateResult, AnswerTokenUpdater,
    DeletableAnswerTokenProperty, DeletablePreparedInstallationConfigProperty, Installation,
    PreparedInstallationConfig, PreparedInstallationConfigCreateResult,
    PreparedInstallationConfigUpdateResult, PreparedInstallationConfigUpdater,
};
use pdm_api_types::remote_updates::RemoteUpdateSummary;
use pdm_api_types::remotes::{RemoteType, TlsProbeOutcome};
use pdm_api_types::resource::{PveResource, RemoteResources, ResourceType, TopEntities};
use pdm_api_types::rrddata::{
    LxcDataPoint, NodeDataPoint, PbsDatastoreDataPoint, PbsNodeDataPoint, PveStorageDataPoint,
    QemuDataPoint,
};
use pdm_api_types::sdn::{ListVnet, ListZone};
use pdm_api_types::{BasicRealmInfo, CertificateInfo};
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

    pub use pve_api_types::{LxcSnapshot, QemuSnapshot};

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

    pub use pve_api_types::{
        CephFlagInfo, CephFlagInfoName, CephFs, CephMds, CephMgr, CephMon, CephPool,
    };

    pub use pve_api_types::ClusterResourceNetworkType;

    pub use pve_api_types::StorageStatus as PveStorageStatus;

    pub use pdm_api_types::subscription::{
        AutoAssignProposal, ClearPendingResult, ProductType, ProposedAssignment, RemoteNodeStatus,
        RemoteSubscriptionState, RemoteSubscriptions, SubscriptionKeyEntry, SubscriptionKeySource,
    };

    pub use pve_api_types::{SdnVnetMacVrf, SdnZoneIpVrf};
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

/// Filter for [`PdmClient::pve_list_storages`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PveListStoragesFilter {
    /// Only list stores which support this content type.
    pub content: Vec<StorageContent>,
    /// Only list stores which are enabled (not disabled in config).
    pub enabled: Option<bool>,
    /// Only list status for  specified storage
    pub storage: Option<String>,
    // If target is different to 'node', we only list shared storages which are accessible on
    // this 'node' and the specified 'target' node.
    pub target: Option<String>,
}

impl<T: HttpApiClient> PdmClient<T> {
    pub async fn list_remotes(&self) -> Result<Vec<Remote>, Error> {
        Ok(self
            .0
            .get("/api2/extjs/remotes/remote")
            .await?
            .expect_json()?
            .data)
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
                "/api2/extjs/remotes/remote",
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
        delete: &[String],
    ) -> Result<(), Error> {
        let path = format!("/api2/extjs/remotes/remote/{remote}");
        let mut request = serde_json::to_value(updater).expect("failed to serialize updater");
        if !delete.is_empty() {
            request["delete"] = serde_json::to_value(delete).expect("failed to serialize delete");
        }
        self.0.put(&path, &request).await?.nodata()?;
        Ok(())
    }

    /// Deletes a remote, with optional flag to handle remote token deletion.
    pub async fn delete_remote(
        &self,
        remote: &str,
        delete_token: Option<bool>,
    ) -> Result<(), Error> {
        let path = ApiPathBuilder::new(format!("/api2/extjs/remotes/remote/{remote}"))
            .maybe_arg("delete-token", &delete_token)
            .build();
        self.0.delete(&path).await?.nodata()?;
        Ok(())
    }

    pub async fn remote_version(
        &self,
        remote: &str,
    ) -> Result<pve_api_types::VersionResponse, proxmox_client::Error> {
        let path = format!("/api2/extjs/remotes/remote/{remote}/version");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    /// Re-probe a configured node's TLS certificate (ignoring the pinned fingerprint),
    /// so a rotated certificate can be detected and the stored fingerprint updated.
    pub async fn remote_probe_certificate(
        &self,
        remote: &str,
        node: &str,
    ) -> Result<TlsProbeOutcome, Error> {
        let path = format!("/api2/extjs/remotes/remote/{remote}/probe-certificate");
        let request = json!({ "node": node });
        Ok(self.0.post(&path, &request).await?.expect_json()?.data)
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
    pub async fn trigger_remote_metric_collection(
        &self,
        remote: Option<&str>,
    ) -> Result<(), proxmox_client::Error> {
        let path = "/api2/extjs/remotes/metric-collection/trigger";

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
    pub async fn get_remote_metric_collection_status(
        &self,
    ) -> Result<Vec<pdm_api_types::RemoteMetricCollectionStatus>, Error> {
        let path = "/api2/extjs/remotes/metric-collection/status";
        Ok(self.0.get(path).await?.expect_json()?.data)
    }

    /// Get PDM node RRD data.
    pub async fn get_pdm_node_rrddata(
        &self,
        mode: RrdMode,
        timeframe: RrdTimeframe,
    ) -> Result<Vec<pdm_api_types::rrddata::PdmNodeDatapoint>, Error> {
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
        let path = ApiPathBuilder::new(format!("/api2/extjs/remotes/remote/{remote}/rrddata"))
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

    pub async fn pve_cluster_updates(&self, remote: &str) -> Result<RemoteUpdateSummary, Error> {
        let url = format!("/api2/extjs/pve/remotes/{remote}/updates");
        Ok(self.0.get(&url).await?.expect_json()?.data)
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

    /// Get the next free VMID on a (possibly remote/external) cluster.
    pub async fn pve_cluster_nextid(
        &self,
        remote: &str,
        target_endpoint: Option<&str>,
    ) -> Result<u32, Error> {
        let query = ApiPathBuilder::new(format!("/api2/extjs/pve/remotes/{remote}/cluster-nextid"))
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

    pub async fn pve_qemu_resume(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
    ) -> Result<RemoteUpid, Error> {
        self.pve_change_guest_status(remote, node, vmid, "qemu", "resume")
            .await
    }

    pub async fn pve_qemu_list_snapshots(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
    ) -> Result<Vec<pve_api_types::QemuSnapshot>, Error> {
        let path = ApiPathBuilder::new(format!(
            "/api2/extjs/pve/remotes/{remote}/qemu/{vmid}/snapshot"
        ))
        .maybe_arg("node", &node)
        .build();
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_qemu_snapshot_create(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
        snapname: &str,
        description: Option<&str>,
        vmstate: Option<bool>,
    ) -> Result<RemoteUpid, Error> {
        let path = format!("/api2/extjs/pve/remotes/{remote}/qemu/{vmid}/snapshot");
        let mut request = json!({ "snapname": snapname });
        if let Some(node) = node {
            request["node"] = node.into();
        }
        if let Some(description) = description {
            request["description"] = description.into();
        }
        if let Some(vmstate) = vmstate {
            request["vmstate"] = vmstate.into();
        }
        Ok(self.0.post(&path, &request).await?.expect_json()?.data)
    }

    pub async fn pve_qemu_snapshot_delete(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
        snapname: &str,
    ) -> Result<RemoteUpid, Error> {
        let path = ApiPathBuilder::new(format!(
            "/api2/extjs/pve/remotes/{remote}/qemu/{vmid}/snapshot/{snapname}"
        ))
        .maybe_arg("node", &node)
        .build();
        Ok(self.0.delete(&path).await?.expect_json()?.data)
    }

    pub async fn pve_qemu_snapshot_rollback(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
        snapname: &str,
        start: Option<bool>,
    ) -> Result<RemoteUpid, Error> {
        let path =
            format!("/api2/extjs/pve/remotes/{remote}/qemu/{vmid}/snapshot/{snapname}/rollback");
        let mut request = json!({});
        if let Some(node) = node {
            request["node"] = node.into();
        }
        if let Some(start) = start {
            request["start"] = start.into();
        }
        Ok(self.0.post(&path, &request).await?.expect_json()?.data)
    }

    /// Update a QEMU snapshot's description. Synchronous on the PVE side (no task).
    /// `None` leaves the description unchanged; `Some("")` clears it.
    pub async fn pve_qemu_snapshot_update_config(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
        snapname: &str,
        description: Option<&str>,
    ) -> Result<(), Error> {
        let path =
            format!("/api2/extjs/pve/remotes/{remote}/qemu/{vmid}/snapshot/{snapname}/config");
        let mut request = json!({});
        if let Some(node) = node {
            request["node"] = node.into();
        }
        if let Some(description) = description {
            request["description"] = description.into();
        }
        self.0.put(&path, &request).await?.nodata()?;
        Ok(())
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

    pub async fn pve_lxc_list_snapshots(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
    ) -> Result<Vec<pve_api_types::LxcSnapshot>, Error> {
        let path = ApiPathBuilder::new(format!(
            "/api2/extjs/pve/remotes/{remote}/lxc/{vmid}/snapshot"
        ))
        .maybe_arg("node", &node)
        .build();
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_lxc_snapshot_create(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
        snapname: &str,
        description: Option<&str>,
    ) -> Result<RemoteUpid, Error> {
        let path = format!("/api2/extjs/pve/remotes/{remote}/lxc/{vmid}/snapshot");
        let mut request = json!({ "snapname": snapname });
        if let Some(node) = node {
            request["node"] = node.into();
        }
        if let Some(description) = description {
            request["description"] = description.into();
        }
        Ok(self.0.post(&path, &request).await?.expect_json()?.data)
    }

    pub async fn pve_lxc_snapshot_delete(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
        snapname: &str,
    ) -> Result<RemoteUpid, Error> {
        let path = ApiPathBuilder::new(format!(
            "/api2/extjs/pve/remotes/{remote}/lxc/{vmid}/snapshot/{snapname}"
        ))
        .maybe_arg("node", &node)
        .build();
        Ok(self.0.delete(&path).await?.expect_json()?.data)
    }

    pub async fn pve_lxc_snapshot_rollback(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
        snapname: &str,
        start: Option<bool>,
    ) -> Result<RemoteUpid, Error> {
        let path =
            format!("/api2/extjs/pve/remotes/{remote}/lxc/{vmid}/snapshot/{snapname}/rollback");
        let mut request = json!({});
        if let Some(node) = node {
            request["node"] = node.into();
        }
        if let Some(start) = start {
            request["start"] = start.into();
        }
        Ok(self.0.post(&path, &request).await?.expect_json()?.data)
    }

    /// Update an LXC snapshot's description. Synchronous on the PVE side (no task).
    /// `None` leaves the description unchanged; `Some("")` clears it.
    pub async fn pve_lxc_snapshot_update_config(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
        snapname: &str,
        description: Option<&str>,
    ) -> Result<(), Error> {
        let path =
            format!("/api2/extjs/pve/remotes/{remote}/lxc/{vmid}/snapshot/{snapname}/config");
        let mut request = json!({});
        if let Some(node) = node {
            request["node"] = node.into();
        }
        if let Some(description) = description {
            request["description"] = description.into();
        }
        self.0.put(&path, &request).await?.nodata()?;
        Ok(())
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

    /// Block until a PDM-local worker task finishes; returns the final status payload.
    ///
    /// The local task-status endpoint (`/nodes/localhost/tasks/{upid}/status`) has no
    /// server-side `wait=1` today, so the helper polls at one-second intervals; sub-second
    /// tasks (e.g. an Apply Pending with an empty queue) settle on the first request. Once a
    /// server-side wait surface lands this method becomes a single GET with no behaviour change
    /// for callers.
    ///
    /// No built-in time bound; wrap in `tokio::time::timeout` if needed. Dropping the future
    /// stops the client-side polling only - the server-side worker keeps running.
    ///
    /// Native-only: the polling loop relies on `tokio::time::sleep`, which is not available on
    /// the wasm32 target the UI builds for.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn wait_for_local_task(&self, upid: &str) -> Result<Value, Error> {
        let path = format!("/api2/extjs/nodes/localhost/tasks/{upid}/status");
        loop {
            let body: Value = self.0.get(&path).await?.expect_json()?.data;
            if body["status"].as_str() != Some("running") {
                return Ok(body);
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
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
        search: Option<&str>,
    ) -> Result<Vec<RemoteResources>, Error> {
        let path = ApiPathBuilder::new("/api2/extjs/resources/list")
            .maybe_arg("max-age", &max_age)
            .arg("resource-type", resource_type)
            .maybe_arg("view", &view)
            .maybe_arg("search", &search)
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

    /// List all keys in the subscription pool. Returns the entries plus the matching
    /// `ConfigDigest` so the caller can chain a digest-aware add / assign / delete back.
    pub async fn list_subscription_keys(
        &self,
    ) -> Result<(Vec<SubscriptionKeyEntry>, Option<ConfigDigest>), Error> {
        let mut res = self
            .0
            .get("/api2/extjs/subscriptions/keys")
            .await?
            .expect_json()?;
        Ok((res.data, res.attribs.remove("digest").map(ConfigDigest)))
    }

    /// Add one or more keys to the pool. See the daemon-side endpoint for the all-or-nothing
    /// validation semantics.
    pub async fn add_subscription_keys(
        &self,
        keys: &[String],
        digest: Option<ConfigDigest>,
    ) -> Result<pdm_api_types::subscription::AddKeysResult, Error> {
        #[derive(Serialize)]
        struct AddArgs<'a> {
            keys: &'a [String],
            #[serde(skip_serializing_if = "Option::is_none")]
            digest: Option<ConfigDigest>,
        }
        Ok(self
            .0
            .post("/api2/extjs/subscriptions/keys", &AddArgs { keys, digest })
            .await?
            .expect_json()?
            .data)
    }

    /// Bind a key to a remote node.
    pub async fn set_subscription_assignment(
        &self,
        key: &str,
        remote: &str,
        node: &str,
        digest: Option<ConfigDigest>,
    ) -> Result<(), Error> {
        #[derive(Serialize)]
        struct AssignArgs<'a> {
            remote: &'a str,
            node: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            digest: Option<ConfigDigest>,
        }
        let path = format!("/api2/extjs/subscriptions/keys/{key}/assignment");
        self.0
            .post(
                &path,
                &AssignArgs {
                    remote,
                    node,
                    digest,
                },
            )
            .await?
            .nodata()
    }

    /// Drop the remote-node binding for a pool key (the inverse of
    /// [`set_subscription_assignment`]).
    pub async fn clear_subscription_assignment(
        &self,
        key: &str,
        digest: Option<ConfigDigest>,
    ) -> Result<(), Error> {
        let path = ApiPathBuilder::new(format!("/api2/extjs/subscriptions/keys/{key}/assignment"))
            .maybe_arg("digest", &digest.map(Value::from))
            .build();
        self.0.delete(&path).await?.nodata()
    }

    /// Remove a key from the pool entirely.
    ///
    /// No digest parameter: deletion is a point-of-no-return operation and the typed-client
    /// surface elsewhere (delete_remote, delete_user, ...) does not round-trip a digest on
    /// DELETE either. External REST callers can still pass `digest` via the URL query if they
    /// want optimistic concurrency on deletion; the server-side endpoint accepts it.
    pub async fn delete_subscription_key(&self, key: &str) -> Result<(), Error> {
        let path = format!("/api2/extjs/subscriptions/keys/{key}");
        self.0.delete(&path).await?.nodata()
    }

    /// Combined remote/node subscription status, filtered to remotes the caller has audit
    /// privilege on.
    pub async fn subscription_node_status(
        &self,
        max_age: Option<u64>,
    ) -> Result<Vec<RemoteNodeStatus>, Error> {
        let path = ApiPathBuilder::new("/api2/extjs/subscriptions/node-status")
            .maybe_arg("max-age", &max_age)
            .build();
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    /// Compute a key-to-node assignment proposal. Apply it with
    /// [`subscription_bulk_assign`].
    pub async fn subscription_auto_assign(&self) -> Result<AutoAssignProposal, Error> {
        Ok(self
            .0
            .post("/api2/extjs/subscriptions/auto-assign", &json!({}))
            .await?
            .expect_json()?
            .data)
    }

    /// Commit a proposal previously returned by [`subscription_auto_assign`]. The server
    /// rejects the call with 409 if either the pool or the live node-status has drifted
    /// since the proposal was computed.
    pub async fn subscription_bulk_assign(
        &self,
        proposal: AutoAssignProposal,
    ) -> Result<Vec<ProposedAssignment>, Error> {
        Ok(self
            .0
            .post(
                "/api2/extjs/subscriptions/bulk-assign",
                &json!({ "proposal": proposal }),
            )
            .await?
            .expect_json()?
            .data)
    }

    /// Push every pending assignment. Returns the worker UPID, or `None` when there is nothing
    /// to do.
    ///
    /// The optional `digest` rejects the call at the API boundary if the pool changed since the
    /// caller last loaded it - the at-API-call-time plan is pinned, but the worker re-reads when
    /// it fires, so a parallel admin edit between API return and worker start is still honoured.
    pub async fn subscription_apply_pending(
        &self,
        digest: Option<ConfigDigest>,
    ) -> Result<Option<String>, Error> {
        #[derive(Serialize)]
        struct Args {
            #[serde(skip_serializing_if = "Option::is_none")]
            digest: Option<ConfigDigest>,
        }
        Ok(self
            .0
            .post("/api2/extjs/subscriptions/apply-pending", &Args { digest })
            .await?
            .expect_json()?
            .data)
    }

    /// Adopt the live subscription on `remote`/`node` into the pool: imports the live key as a
    /// new pool entry bound to (remote, node) without touching the remote. Refuses if (remote,
    /// node) already has a pool entry bound to it. See the server endpoint docs for the full
    /// per-sub-case semantics (existing-unbound, existing-bound-elsewhere, not-in-pool).
    pub async fn subscription_adopt_key(
        &self,
        remote: &str,
        node: &str,
        digest: Option<ConfigDigest>,
    ) -> Result<(), Error> {
        #[derive(Serialize)]
        struct AdoptArgs<'a> {
            remote: &'a str,
            node: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            digest: Option<ConfigDigest>,
        }
        self.0
            .post(
                "/api2/extjs/subscriptions/adopt-key",
                &AdoptArgs {
                    remote,
                    node,
                    digest,
                },
            )
            .await?
            .nodata()
    }

    /// Adopt every foreign live subscription that the caller can modify, in one transaction.
    /// Returns the list of `(remote, node, key)` tuples that were imported into the pool;
    /// candidates the caller has no `PRIV_RESOURCE_MODIFY` on (or that fail validation, or that
    /// are already bound elsewhere in the pool) are silently skipped. See the server endpoint
    /// docs for the full skip rules.
    pub async fn subscription_adopt_all(
        &self,
        digest: Option<ConfigDigest>,
    ) -> Result<Vec<pdm_api_types::subscription::AdoptedEntry>, Error> {
        #[derive(Serialize)]
        struct AdoptAllArgs {
            #[serde(skip_serializing_if = "Option::is_none")]
            digest: Option<ConfigDigest>,
        }
        Ok(self
            .0
            .post(
                "/api2/extjs/subscriptions/adopt-all",
                &AdoptAllArgs { digest },
            )
            .await?
            .expect_json()?
            .data)
    }

    /// Queue a clear for the subscription on `remote`/`node`. Apply Pending later removes the
    /// subscription from the node so the key can be reassigned elsewhere; Discard Pending
    /// undoes the queueing without touching the remote. Returns `BAD_REQUEST` if no pool entry
    /// is bound to (remote, node); callers must run Adopt Key first to import a foreign
    /// subscription.
    pub async fn subscription_queue_clear(
        &self,
        remote: &str,
        node: &str,
        digest: Option<ConfigDigest>,
    ) -> Result<(), Error> {
        #[derive(Serialize)]
        struct ClearArgs<'a> {
            remote: &'a str,
            node: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            digest: Option<ConfigDigest>,
        }
        self.0
            .post(
                "/api2/extjs/subscriptions/queue-clear",
                &ClearArgs {
                    remote,
                    node,
                    digest,
                },
            )
            .await?
            .nodata()
    }

    /// Drop a queued Clear Key on `remote`/`node` while keeping the pool binding. Used by the
    /// per-node Revert action; the global Discard Pending path scrubs every pending change at
    /// once.
    pub async fn subscription_revert_pending_clear(
        &self,
        remote: &str,
        node: &str,
        digest: Option<ConfigDigest>,
    ) -> Result<(), Error> {
        #[derive(Serialize)]
        struct Args<'a> {
            remote: &'a str,
            node: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            digest: Option<ConfigDigest>,
        }
        self.0
            .post(
                "/api2/extjs/subscriptions/revert-pending-clear",
                &Args {
                    remote,
                    node,
                    digest,
                },
            )
            .await?
            .nodata()
    }

    /// Trigger a fresh shop-side subscription check on `remote`/`node`. Equivalent to the
    /// per-product "Check" button: drives `update_subscription(force=true)` and invalidates the
    /// remote's cached subscription state so the next `subscription_node_status` reflects the
    /// new verdict.
    pub async fn subscription_check(&self, remote: &str, node: &str) -> Result<(), Error> {
        #[derive(Serialize)]
        struct Args<'a> {
            remote: &'a str,
            node: &'a str,
        }
        self.0
            .post("/api2/extjs/subscriptions/check", &Args { remote, node })
            .await?
            .nodata()
    }

    /// Clear every pending assignment in one bulk transaction; returns the count of cleared
    /// entries.
    pub async fn subscription_clear_pending(
        &self,
        digest: Option<ConfigDigest>,
    ) -> Result<u32, Error> {
        #[derive(Serialize)]
        struct Args {
            #[serde(skip_serializing_if = "Option::is_none")]
            digest: Option<ConfigDigest>,
        }
        let result: types::ClearPendingResult = self
            .0
            .post("/api2/extjs/subscriptions/clear-pending", &Args { digest })
            .await?
            .expect_json()?
            .data;
        Ok(result.cleared)
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

    /// List storages for a given PVE remote node.
    ///
    /// The storages can be filtered using the `filter` parameter, for details see
    /// [`PveListStoragesFilter`]. If `include_supported_disk_image_formats` is set
    /// to true, the result will include information about supported disk image types
    /// for each storage.
    pub async fn pve_list_storages(
        &self,
        remote: &str,
        node: &str,
        filter: PveListStoragesFilter,
        include_supported_disk_image_formats: bool,
    ) -> Result<Vec<StorageInfo>, Error> {
        let mut builder = ApiPathBuilder::new(format!(
            "/api2/extjs/pve/remotes/{remote}/nodes/{node}/storage"
        ))
        .arg("format", include_supported_disk_image_formats)
        .maybe_arg("enabled", &filter.enabled)
        .maybe_arg("storage", &filter.storage)
        .maybe_arg("target", &filter.target);

        for ty in filter.content {
            builder = builder.arg("content", ty);
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

    pub async fn pve_sdn_vnet_get_mac_vrf(
        &self,
        remote: &str,
        node: &str,
        vnet: &str,
    ) -> Result<Vec<SdnVnetMacVrf>, Error> {
        let path =
            format!("/api2/extjs/pve/remotes/{remote}/nodes/{node}/sdn/vnets/{vnet}/mac-vrf");
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
            .get("/api2/extjs/remotes/updates/summary")
            .await?
            .expect_json()?
            .data)
    }

    /// Refresh remote update summary.
    pub async fn refresh_remote_update_summary(&self) -> Result<pdm_api_types::UPID, Error> {
        Ok(self
            .0
            .post_without_body("/api2/extjs/remotes/updates/refresh")
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

    /// Get the list of views.
    pub async fn list_views(&self) -> Result<Vec<pdm_api_types::views::ViewConfig>, Error> {
        Ok(self
            .0
            .get("/api2/extjs/config/views")
            .await?
            .expect_json()?
            .data)
    }

    /// Retrieves all known installations done by auto-installer.
    pub async fn get_autoinst_installations(&self) -> Result<Vec<Installation>, Error> {
        Ok(self
            .0
            .get("/api2/extjs/auto-install/installations")
            .await?
            .expect_json()?
            .data)
    }

    /// Deletes a saved auto-installation.
    ///
    /// # Parameters
    ///
    /// * `id` - ID of the entry to delete. Must be percent-encoded.
    pub async fn delete_autoinst_installation(&self, id: &str) -> Result<(), Error> {
        self.0
            .delete(&format!("/api2/extjs/auto-install/installations/{id}"))
            .await?
            .nodata()?;
        Ok(())
    }

    /// Retrieves all prepared answer configurations.
    pub async fn get_autoinst_prepared_answers(
        &self,
    ) -> Result<Vec<PreparedInstallationConfig>, Error> {
        Ok(self
            .0
            .get("/api2/extjs/auto-install/prepared")
            .await?
            .expect_json()?
            .data)
    }

    /// Adds a new prepared answer file configuration for automated installations.
    ///
    /// # Arguments
    ///
    /// * `config` - Answer to create.
    /// * `root_password` - Optional root password to set for this answer.
    ///
    /// # Returns
    ///
    /// The newly created configuration, including the generated secret.
    pub async fn add_autoinst_prepared_answer(
        &self,
        config: &PreparedInstallationConfig,
        root_password: Option<&str>,
    ) -> Result<PreparedInstallationConfigCreateResult, Error> {
        #[derive(Serialize)]
        #[serde(rename_all = "kebab-case")]
        struct CreatePreparedAnswer<'a> {
            #[serde(flatten)]
            config: &'a PreparedInstallationConfig,
            #[serde(skip_serializing_if = "Option::is_none")]
            root_password: Option<&'a str>,
        }

        Ok(self
            .0
            .post(
                "/api2/extjs/auto-install/prepared",
                &CreatePreparedAnswer {
                    config,
                    root_password,
                },
            )
            .await?
            .expect_json()?
            .data)
    }

    /// Update an existing prepared answer file configuration for automated installations.
    ///
    /// # Arguments
    ///
    /// * `id` - ID of the entry to delete. Must be percent-encoded.
    /// * `updater` - Field values to update.
    /// * `root_password` - Optional root password to set for this answer.
    /// * `delete` - List of properties to delete.
    pub async fn update_autoinst_prepared_answer(
        &self,
        id: &str,
        updater: &PreparedInstallationConfigUpdater,
        root_password: Option<&str>,
        delete: &[DeletablePreparedInstallationConfigProperty],
    ) -> Result<PreparedInstallationConfigUpdateResult, Error> {
        #[derive(Serialize)]
        #[serde(rename_all = "kebab-case")]
        struct UpdatePreparedAnswer<'a> {
            #[serde(flatten)]
            updater: &'a PreparedInstallationConfigUpdater,
            #[serde(skip_serializing_if = "Option::is_none")]
            root_password: Option<&'a str>,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            delete: Vec<String>,
        }

        let delete = delete
            .iter()
            .map(DeletablePreparedInstallationConfigProperty::to_string)
            .collect();

        Ok(self
            .0
            .put(
                &format!("/api2/extjs/auto-install/prepared/{id}"),
                &UpdatePreparedAnswer {
                    updater,
                    root_password,
                    delete,
                },
            )
            .await?
            .expect_json()?
            .data)
    }

    /// Deletes a prepared answer for automated installations.
    ///
    /// # Parameters
    ///
    /// * `id` - ID of the entry to delete. Must be percent-encoded.
    pub async fn delete_autoinst_prepared_answer(&self, id: &str) -> Result<(), Error> {
        self.0
            .delete(&format!("/api2/extjs/auto-install/prepared/{id}"))
            .await?
            .nodata()?;
        Ok(())
    }

    /// Retrieves all access tokens for the auto-installer server.
    pub async fn get_autoinst_tokens(&self) -> Result<Vec<AnswerToken>, Error> {
        Ok(self
            .0
            .get("/api2/extjs/auto-install/tokens")
            .await?
            .expect_json()?
            .data)
    }

    /// Adds a new access token for authenticating requests from the automated installer.
    ///
    /// # Parameters
    ///
    /// * `id` - Name of the token to create.
    /// * `comment` - Optional comment for the token.
    /// * `enabled` - Whether this token is enabled.
    /// * `expire_at` - Optional expiration date for this token.
    pub async fn add_autoinst_token(
        &self,
        id: &str,
        comment: Option<String>,
        enabled: Option<bool>,
        expire_at: Option<i64>,
    ) -> Result<AnswerTokenCreateResult, Error> {
        #[derive(Serialize)]
        #[serde(rename_all = "kebab-case")]
        struct CreateTokenRequest<'a> {
            id: &'a str,
            comment: &'a Option<String>,
            enabled: Option<bool>,
            expire_at: Option<i64>,
        }

        Ok(self
            .0
            .post(
                "/api2/extjs/auto-install/tokens",
                &CreateTokenRequest {
                    id,
                    comment: &comment,
                    enabled,
                    expire_at,
                },
            )
            .await?
            .expect_json::<AnswerTokenCreateResult>()?
            .data)
    }

    /// Updates an existing access token for authenticating requests from the automated installer.
    ///
    /// # Parameters
    ///
    /// * `id` - Name of the token to update.
    /// * `updater` - Fields to update.
    /// * `delete` - Fields to delete.
    pub async fn update_autoinst_token(
        &self,
        id: &str,
        updater: &AnswerTokenUpdater,
        delete: &[DeletableAnswerTokenProperty],
        regenerate_secret: bool,
    ) -> Result<AnswerTokenUpdateResult, Error> {
        #[derive(Serialize)]
        #[serde(rename_all = "kebab-case")]
        struct UpdateToken<'a> {
            #[serde(flatten)]
            updater: &'a AnswerTokenUpdater,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            delete: Vec<String>,
            regenerate_secret: bool,
        }

        let delete = delete
            .iter()
            .map(DeletableAnswerTokenProperty::to_string)
            .collect();

        Ok(self
            .0
            .put(
                &format!("/api2/extjs/auto-install/tokens/{id}"),
                &UpdateToken {
                    updater,
                    delete,
                    regenerate_secret,
                },
            )
            .await?
            .expect_json::<AnswerTokenUpdateResult>()?
            .data)
    }

    /// Deletes an access token used for authenticating automated installations.
    ///
    /// # Parameters
    ///
    /// * `id` - Name of the token to delete.
    pub async fn delete_autoinst_token(&self, id: &str) -> Result<(), Error> {
        self.0
            .delete(&format!("/api2/extjs/auto-install/tokens/{id}"))
            .await?
            .nodata()?;
        Ok(())
    }

    /// Get the current certificat's information for the PDM host itself.
    pub async fn certificate_info(&self) -> Result<Vec<CertificateInfo>, Error> {
        Ok(self
            .0
            .get("/api2/extjs/nodes/localhost/certificates/info")
            .await?
            .expect_json::<Vec<CertificateInfo>>()?
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
