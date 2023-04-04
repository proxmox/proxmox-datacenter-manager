//! Proxmox Datacenter Manager API client.

use anyhow::{bail, format_err, Error};
use openssl::x509;
use serde_json::json;

use proxmox_client::Environment;

use pdm_api_types::{ConfigurationState, Remote};

pub struct Client<E: Environment> {
    client: proxmox_client::HyperClient<E>,
}

impl<E> Client<E>
where
    E: Environment,
    E::Error: From<anyhow::Error>,
    anyhow::Error: From<E::Error>,
{
    pub fn new(env: E, server: &str, options: Options) -> Result<Self, E::Error> {
        use proxmox_client::TlsOptions;

        let tls_options = match options.callback {
            Some(cb) => TlsOptions::Callback(cb),
            None => TlsOptions::default(),
        };

        let client = proxmox_client::HyperClient::with_options(
            server
                .parse()
                .map_err(|err| format_err!("bad address: {server:?} - {err}"))?,
            env,
            tls_options,
            options.http_options,
        )?;

        Ok(Self { client })
    }

    pub async fn login(&self) -> Result<(), Error> {
        self.client.login().await?;
        Ok(())
    }

    pub async fn list_remotes(&self) -> Result<Vec<Remote>, Error> {
        Ok(self
            .client
            .get("/api2/extjs/remotes")
            .await?
            .into_data_or_err()?)
    }

    pub async fn add_remote(&self, remote: &Remote) -> Result<(), Error> {
        self.client
            .post::<_, ()>("/api2/extjs/remotes", remote)
            .await?;
        Ok(())
    }

    pub async fn update_remote(
        &self,
        remote: &str,
        updater: &pdm_api_types::PveRemoteUpdater,
    ) -> Result<(), Error> {
        let path = format!("/api2/extjs/remotes/{remote}");
        self.client.put(&path, updater).await?.nodata()?;
        Ok(())
    }

    pub async fn remove_remote(&self, remote: &str) -> Result<(), Error> {
        let path = format!("/api2/extjs/remotes/{remote}");
        self.client.delete(&path).await?.nodata()?;
        Ok(())
    }

    pub async fn remote_version(
        &self,
        remote: &str,
    ) -> Result<pve_client::types::VersionResponse, Error> {
        let path = format!("/api2/extjs/remotes/{remote}/version");
        Ok(self.client.get(&path).await?.into_data_or_err()?)
    }

    pub async fn list_user_tfa(
        &self,
        userid: &str,
    ) -> Result<Vec<proxmox_tfa::TypedTfaInfo>, Error> {
        let path = format!("/api2/extjs/access/tfa/{userid}");
        Ok(self.client.get(&path).await?.into_data_or_err()?)
    }

    pub async fn remove_tfa_entry(
        &self,
        userid: &str,
        password: Option<&str>,
        id: &str,
    ) -> Result<(), Error> {
        let path = format!("/api2/extjs/access/tfa/{userid}/{id}");

        let mut request = json!({});
        if let Some(pw) = password {
            request["password"] = pw.into();
        }

        self.client
            .delete_with_body(&path, &request)
            .await?
            .nodata()?;
        Ok(())
    }

    pub async fn add_recovery_keys(
        &self,
        userid: &str,
        password: Option<&str>,
        description: &str,
    ) -> Result<Vec<String>, Error> {
        let path = format!("/api2/extjs/access/tfa/{userid}");

        let result: proxmox_tfa::TfaUpdateInfo = self
            .client
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
            .into_data_or_err()?;

        if result.recovery.is_empty() {
            bail!("api returned empty list of recovery keys");
        }

        Ok(result.recovery)
    }

    pub async fn pve_list_nodes(
        &self,
        remote: &str,
    ) -> Result<Vec<pve_client::types::ClusterNodeIndexResponse>, Error> {
        let path = format!("/api2/extjs/pve/{remote}/nodes");
        Ok(self.client.get(&path).await?.into_data_or_err()?)
    }

    pub async fn pve_cluster_resources(
        &self,
        remote: &str,
        kind: Option<pve_client::types::ClusterResourceKind>,
    ) -> Result<Vec<pve_client::types::ClusterResource>, Error> {
        let mut query = format!("/api2/extjs/pve/{remote}/resources");
        let mut sep = '?';
        pve_client::helpers::add_query_arg(&mut query, &mut sep, "kind", &kind);
        Ok(self.client.get(&query).await?.into_data_or_err()?)
    }

    pub async fn pve_list_qemu(
        &self,
        remote: &str,
        node: Option<&str>,
    ) -> Result<Vec<pve_client::types::VmEntry>, Error> {
        let path = format!("/api2/extjs/pve/{remote}/qemu");
        let request = match node {
            None => json!({}),
            Some(node) => json!({ "node": node }),
        };
        Ok(self
            .client
            .get_with_body(&path, &request)
            .await?
            .into_data_or_err()?)
    }

    pub async fn pve_list_lxc(
        &self,
        remote: &str,
        node: Option<&str>,
    ) -> Result<Vec<pve_client::types::VmEntry>, Error> {
        let path = format!("/api2/extjs/pve/{remote}/lxc");
        let request = match node {
            None => json!({}),
            Some(node) => json!({ "node": node }),
        };
        Ok(self
            .client
            .get_with_body(&path, &request)
            .await?
            .into_data_or_err()?)
    }

    pub async fn pve_qemu_config(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u64,
        state: ConfigurationState,
        snapshot: Option<&str>,
    ) -> Result<pve_client::types::QemuConfig, Error> {
        let path = format!("/api2/extjs/pve/{remote}/qemu/{vmid}/config");
        let mut request = json!({
            "state": state,
        });
        if let Some(node) = node {
            request["node"] = node.into();
        }
        if let Some(snapshot) = snapshot {
            request["snapshot"] = snapshot.into();
        }
        Ok(self
            .client
            .get_with_body(&path, &request)
            .await?
            .into_data_or_err()?)
    }

    pub async fn pve_lxc_config(
        &self,
        remote: &str,
        node: Option<&str>,
        vmid: u64,
        state: ConfigurationState,
        snapshot: Option<&str>,
    ) -> Result<pve_client::types::LxcConfig, Error> {
        let path = format!("/api2/extjs/pve/{remote}/lxc/{vmid}/config");
        let mut request = json!({
            "state": state,
        });
        if let Some(node) = node {
            request["node"] = node.into();
        }
        if let Some(snapshot) = snapshot {
            request["snapshot"] = snapshot.into();
        }
        Ok(self
            .client
            .get_with_body(&path, &request)
            .await?
            .into_data_or_err()?)
    }
}

#[derive(Default)]
// TODO: Merge this with pbs-client's stuff
pub struct Options {
    /// Set a TLS verification callback.
    callback:
        Option<Box<dyn Fn(bool, &mut x509::X509StoreContextRef) -> bool + Send + Sync + 'static>>,

    /// `proxmox_http` based options.
    http_options: proxmox_http::HttpOptions,
}

impl Options {
    /// New default instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a TLS verification callback.
    pub fn tls_callback<F>(mut self, cb: F) -> Self
    where
        F: Fn(bool, &mut x509::X509StoreContextRef) -> bool + Send + Sync + 'static,
    {
        self.callback = Some(Box::new(cb));
        self
    }

    /// Set the HTTP related options.
    pub fn http_options(mut self, http_options: proxmox_http::HttpOptions) -> Self {
        self.http_options = http_options;
        self
    }
}

#[derive(serde::Serialize)]
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
