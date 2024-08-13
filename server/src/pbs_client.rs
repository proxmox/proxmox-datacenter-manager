//! Manage PBS instances.
//!
//! Within PDM we do not need the code for creating and streaming backups and just want some basic
//! API calls. This is a more organized client than what we get via the `pdm-client` crate within
//! the PBS repo, which is huge and messy...

use anyhow::{bail, format_err}; // don't import Error as default error in here

use proxmox_client::{Error, HttpApiClient, TlsOptions};
use proxmox_section_config::typed::SectionConfigData;

use pdm_api_types::remotes::{Remote, RemoteType};

pub fn get_remote<'a>(
    config: &'a SectionConfigData<Remote>,
    id: &str,
) -> Result<&'a Remote, anyhow::Error> {
    let remote = crate::api::remotes::get_remote(config, id)?;
    if remote.ty != RemoteType::Pbs {
        bail!("remote {id:?} is not a pbs remote");
    }
    Ok(remote)
}

pub fn connect(remote: &Remote) -> Result<PbsClient, anyhow::Error> {
    let node = remote
        .nodes
        .first()
        .ok_or_else(|| format_err!("no nodes configured for remote"))?;

    let mut options = TlsOptions::default();
    if let Some(fp) = &node.fingerprint {
        options = TlsOptions::parse_fingerprint(fp)?;
    }

    let uri = format!("https://{}:8007", node.hostname).parse()?;
    let client = proxmox_client::Client::with_options(uri, options, Default::default())?;

    client.set_authentication(proxmox_client::Token {
        userid: remote.authid.to_string(),
        prefix: "PBSAPIToken".to_string(),
        value: remote.token.to_string(),
        perl_compat: false,
    });

    Ok(PbsClient(client))
}

pub fn connect_to_remote(
    config: &SectionConfigData<Remote>,
    id: &str,
) -> Result<PbsClient, anyhow::Error> {
    connect(get_remote(config, id)?)
}

pub struct PbsClient(pub proxmox_client::Client);

impl std::ops::Deref for PbsClient {
    type Target = proxmox_client::Client;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for PbsClient {
    fn deref_mut(&mut self) -> &mut proxmox_client::Client {
        &mut self.0
    }
}

impl PbsClient {
    /// API version details, including some parts of the global datacenter config.
    pub async fn version(&self) -> Result<pve_api_types::VersionResponse, Error> {
        Ok(self.0.get("/api2/extjs/version").await?.expect_json()?.data)
    }

    /// List the datastores.
    pub async fn list_datastores(&self) -> Result<Vec<pbs_api_types::DataStoreConfig>, Error> {
        let path = "/api2/extjs/config/datastore";
        Ok(self.0.get(path).await?.expect_json()?.data)
    }
}
