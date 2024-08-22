//! Proxmox Datacenter Manager API client.

use std::collections::HashMap;
use std::time::Duration;

use pdm_api_types::rrddata::{LxcDataPoint, NodeDataPoint, QemuDataPoint};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use proxmox_client::{Error, HttpApiClient};
use proxmox_rrd::api_types::{RRDMode, RRDTimeFrame};

use types::*;
/// For convenience we reexport all the api types the client uses.
pub mod types {
    pub use proxmox_access_control::types::{User, UserWithTokens};

    pub use pdm_api_types::remotes::Remote;
    pub use pdm_api_types::{AclListItem, Authid, ConfigurationState, RemoteUpid};
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

    pub async fn add_remote(&self, remote: &Remote) -> Result<(), proxmox_client::Error> {
        self.0.post("/api2/extjs/remotes", remote).await?.nodata()
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

    pub async fn read_user(&self, user: &str) -> Result<Vec<User>, Error> {
        let path = format!("/api2/extjs/access/users/{user}");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn list_users(&self, include_api_tokens: bool) -> Result<Vec<UserWithTokens>, Error> {
        let mut path = "/api2/extjs/access/users".to_string();
        add_query_arg(
            &mut path,
            &mut '?',
            "include_tokens",
            &Some(include_api_tokens),
        );
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn create_user(&self, config: &User, password: Option<&str>) -> Result<(), Error> {
        #[derive(Serialize)]
        struct CreateUser<'a> {
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
        let mut path = format!("/api2/extjs/access/tfa/{userid}/{id}");
        add_query_arg(&mut path, &mut '?', "password", &password);
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

    pub async fn pve_list_nodes(
        &self,
        remote: &str,
    ) -> Result<Vec<pve_api_types::ClusterNodeIndexResponse>, Error> {
        let path = format!("/api2/extjs/pve/{remote}/nodes");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_node_rrddata(
        &self,
        remote: &str,
        node: &str,
        mode: RRDMode,
        timeframe: RRDTimeFrame,
    ) -> Result<Vec<NodeDataPoint>, Error> {
        let path = format!(
            "/api2/extjs/pve/{remote}/nodes/{node}/rrddata?cf={mode}&timeframe={timeframe}"
        );
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_cluster_resources(
        &self,
        remote: &str,
        kind: Option<pve_api_types::ClusterResourceKind>,
    ) -> Result<Vec<pve_api_types::ClusterResource>, Error> {
        let mut query = format!("/api2/extjs/pve/{remote}/resources");
        add_query_arg(&mut query, &mut '?', "kind", &kind);
        Ok(self.0.get(&query).await?.expect_json()?.data)
    }

    pub async fn pve_list_qemu(
        &self,
        remote: &str,
        node: Option<&str>,
    ) -> Result<Vec<pve_api_types::VmEntry>, Error> {
        let mut path = format!("/api2/extjs/pve/{remote}/qemu");
        add_query_arg(&mut path, &mut '?', "node", &node);
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_list_lxc(
        &self,
        remote: &str,
        node: Option<&str>,
    ) -> Result<Vec<pve_api_types::VmEntry>, Error> {
        let mut path = format!("/api2/extjs/pve/{remote}/lxc");
        add_query_arg(&mut path, &mut '?', "node", &node);
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
        let mut path = format!("/api2/extjs/pve/{remote}/qemu/{vmid}/config");
        let mut sep = '?';
        add_query_arg(&mut path, &mut sep, "state", &Some(&state));
        add_query_arg(&mut path, &mut sep, "node", &node);
        add_query_arg(&mut path, &mut sep, "snapshot", &snapshot);
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
        let path = format!("/api2/extjs/pve/{remote}/{vmtype}/{vmid}/{action}");
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

    pub async fn pve_qemu_remote_migrate(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
        target: String,
        params: RemoteMigrateQemu,
    ) -> Result<RemoteUpid, Error> {
        let path = format!("/api2/extjs/pve/{remote}/qemu/{vmid}/remote-migrate");
        let mut request = serde_json::to_value(&params).expect("failed to build json string");
        request["target"] = target.into();
        if let Some(node) = node {
            request["node"] = node.into();
        }
        Ok(self.0.post(&path, &request).await?.expect_json()?.data)
    }

    pub async fn pve_qemu_rrddata(
        &self,
        remote: &str,
        vmid: u32,
        mode: RRDMode,
        timeframe: RRDTimeFrame,
    ) -> Result<Vec<QemuDataPoint>, Error> {
        let path =
            format!("/api2/extjs/pve/{remote}/qemu/{vmid}/rrddata?cf={mode}&timeframe={timeframe}");
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
        let mut path = format!("/api2/extjs/pve/{remote}/lxc/{vmid}/config");
        let mut sep = '?';
        add_query_arg(&mut path, &mut sep, "node", &node);
        add_query_arg(&mut path, &mut sep, "state", &Some(&state));
        add_query_arg(&mut path, &mut sep, "snapshot", &snapshot);
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

    pub async fn pve_lxc_remote_migrate(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u32,
        target: String,
        params: RemoteMigrateLxc,
    ) -> Result<RemoteUpid, Error> {
        let path = format!("/api2/extjs/pve/{remote}/lxc/{vmid}/remote-migrate");
        let mut request = serde_json::to_value(&params).expect("failed to build json string");
        request["target"] = target.into();
        if let Some(node) = node {
            request["node"] = node.into();
        }
        Ok(self.0.post(&path, &request).await?.expect_json()?.data)
    }

    pub async fn pve_lxc_rrddata(
        &self,
        remote: &str,
        vmid: u32,
        mode: RRDMode,
        timeframe: RRDTimeFrame,
    ) -> Result<Vec<LxcDataPoint>, Error> {
        let path =
            format!("/api2/extjs/pve/{remote}/lxc/{vmid}/rrddata?cf={mode}&timeframe={timeframe}");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_list_tasks(
        &self,
        remote: &str,
        node: Option<&str>,
    ) -> Result<Vec<pve_api_types::ListTasksResponse>, Error> {
        let mut query = format!("/api2/extjs/pve/{remote}/tasks");
        let mut sep = '?';
        pve_api_types::client::add_query_arg(&mut query, &mut sep, "node", &node);
        Ok(self.0.get(&query).await?.expect_json()?.data)
    }

    pub async fn pve_stop_task(&self, remote: &str, upid: &str) -> Result<(), Error> {
        let path = format!("/api2/extjs/pve/{remote}/tasks/{upid}");
        #[allow(clippy::unit_arg)]
        Ok(self.0.delete(&path).await?.expect_json()?.data)
    }

    pub async fn pve_task_status(
        &self,
        upid: &RemoteUpid,
    ) -> Result<pve_api_types::TaskStatus, Error> {
        let remote = upid.remote();
        let upid = upid.to_string();
        let path = format!("/api2/extjs/pve/{remote}/tasks/{upid}/status");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pve_wait_for_task(
        &self,
        upid: &RemoteUpid,
    ) -> Result<pve_api_types::TaskStatus, Error> {
        let remote = upid.remote();
        let upid = upid.to_string();
        let path = format!("/api2/extjs/pve/{remote}/tasks/{upid}/status?wait=1");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn read_acl(
        &self,
        path: Option<&str>,
        exact: bool,
    ) -> Result<(Vec<AclListItem>, Option<ConfigDigest>), Error> {
        let mut query = format!("/api2/extjs/access/acl?exact={}", exact as u8);
        let mut sep = '?';
        pve_api_types::client::add_query_arg(&mut query, &mut sep, "path", &path);
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
        let path = format!("/api2/extjs/pbs/{remote}/datastore");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    pub async fn pbs_list_snapshots(
        &self,
        remote: &str,
        store: &str,
        namespace: Option<&str>,
    ) -> Result<Vec<pbs_api_types::SnapshotListItem>, Error> {
        let mut path = format!("/api2/extjs/pbs/{remote}/datastore/{store}/snapshots");
        add_query_arg(&mut path, &mut '?', "ns", &namespace);
        Ok(self.0.get(&path).await?.expect_json()?.data)
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

    let mut output = String::new();
    for (from, to) in mapping.iter() {
        if !output.is_empty() {
            output.reserve(from.len() + to.len() + 2);
            output.push(',');
        } else {
            output.reserve(from.len() + to.len() + 1);
        }
        output.push_str(from);
        output.push(':');
        output.push_str(to);
    }

    serializer.serialize_str(&output)
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

/// Add an optional string parameter to the query, and if it was added, change `separator` to `&`.
pub fn add_query_arg<T>(query: &mut String, separator: &mut char, name: &str, value: &Option<T>)
where
    T: std::fmt::Display,
{
    if let Some(value) = value {
        query.push(*separator);
        *separator = '&';
        query.push_str(name);
        query.push('=');
        query.extend(percent_encoding::percent_encode(
            value.to_string().as_bytes(),
            percent_encoding::NON_ALPHANUMERIC,
        ));
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
