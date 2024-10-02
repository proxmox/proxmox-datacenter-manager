//! Manage PBS instances.
//!
//! Within PDM we do not need the code for creating and streaming backups and just want some basic
//! API calls. This is a more organized client than what we get via the `pdm-client` crate within
//! the PBS repo, which is huge and messy...

use anyhow::bail; // don't import Error as default error in here
use serde::Deserialize;

use proxmox_client::{Error, HttpApiClient};
use proxmox_router::stream::JsonRecords;
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

pub async fn connect_or_login(remote: &Remote) -> Result<PbsClient, anyhow::Error> {
    let client = crate::connection::connect_or_login(remote).await?;
    Ok(PbsClient(client))
}

pub fn connect(remote: &Remote) -> Result<PbsClient, anyhow::Error> {
    let client = crate::connection::connect(remote)?;
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

    /// List a datastore's snapshots.
    pub async fn list_snapshots(
        &self,
        datastore: &str,
        namespace: Option<&str>,
    ) -> Result<JsonRecords<pbs_api_types::SnapshotListItem>, anyhow::Error> {
        let mut path = format!("/api2/extjs/admin/datastore/{datastore}/snapshots");
        add_query_arg(&mut path, &mut '?', "ns", &namespace);
        let response = self
            .0
            .streaming_request(http::Method::GET, &path, None::<()>)
            .await?;

        let body = response
            .body
            .ok_or_else(|| Error::Other("missing response body"))?;

        if response.status == 200 {
            if response
                .content_type
                .is_some_and(|c| c.starts_with("application/json-seq"))
            {
                Ok(JsonRecords::from_body(body))
            } else {
                let response: JsonData<_> =
                    serde_json::from_slice(&hyper::body::to_bytes(body).await?)?;
                Ok(JsonRecords::from_vec(response.data))
            }
        } else {
            let data = hyper::body::to_bytes(body).await?;
            let error = String::from_utf8_lossy(&data).into_owned();
            Err(anyhow::Error::msg(error))
        }
    }
}

#[derive(Deserialize)]
struct JsonData<T> {
    data: T,
}

/// Add an optional string parameter to the query, and if it was added, change `separator` to `&`.
fn add_query_arg<T>(query: &mut String, separator: &mut char, name: &str, value: &Option<T>)
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
