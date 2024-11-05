//! Manage PBS instances.
//!
//! Within PDM we do not need the code for creating and streaming backups and just want some basic
//! API calls. This is a more organized client than what we get via the `pdm-client` crate within
//! the PBS repo, which is huge and messy...

use anyhow::bail; // don't import Error as default error in here
use serde::Deserialize;

use proxmox_client::{Error, HttpApiClient};
use proxmox_router::stream::JsonRecords;
use proxmox_schema::api;
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

pub async fn connect_or_login(remote: &Remote) -> Result<Box<PbsClient>, anyhow::Error> {
    crate::connection::make_pbs_client_and_login(remote).await
}

pub fn connect(remote: &Remote) -> Result<Box<PbsClient>, anyhow::Error> {
    crate::connection::make_pbs_client(remote)
}

pub fn connect_to_remote(
    config: &SectionConfigData<Remote>,
    id: &str,
) -> Result<Box<PbsClient>, anyhow::Error> {
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

#[api]
/// Create token response.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct CreateTokenResponse {
    /// The token id.
    pub tokenid: String,

    /// API token value used for authentication.
    pub value: String,
}

#[api]
/// Create token parameters.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct CreateToken {
    /// The comment
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Enable the token
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable: Option<bool>,
    /// Set a token expiration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expire: Option<i64>,
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

    /// create a pbs token
    pub async fn create_token(
        &self,
        userid: &str,
        tokenid: &str,
        params: CreateToken,
    ) -> Result<CreateTokenResponse, Error> {
        let path = format!("/api2/extjs/access/users/{userid}/token/{tokenid}");
        Ok(self.0.post(&path, &params).await?.expect_json()?.data)
    }

    /// Return the status the Proxmox Backup Server instance
    pub async fn node_status(&self) -> Result<pbs_api_types::NodeStatus, Error> {
        let path = "/api2/extjs/nodes/localhost/status";
        Ok(self.0.get(path).await?.expect_json()?.data)
    }

    /// Return the datastore status
    pub async fn datastore_status(
        &self,
        datastore: &str,
    ) -> Result<pbs_api_types::DataStoreStatus, Error> {
        let path = format!("/api2/extjs/admin/datastore/{datastore}/status");
        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    /// Return datastore usages and estimates
    pub async fn datastore_usage(
        &self,
    ) -> Result<Vec<pbs_api_types::DataStoreStatusListItem>, Error> {
        let path = "/api2/extjs/status/datastore-usage";
        Ok(self.0.get(path).await?.expect_json()?.data)
    }

    /// Return backup server metrics.
    pub async fn metrics(
        &self,
        history: Option<bool>,
        start_time: Option<i64>,
    ) -> Result<pbs_api_types::Metrics, Error> {
        let mut path = "/api2/extjs/status/metrics".to_string();
        let mut sep = '?';
        add_query_arg(&mut path, &mut sep, "history", &history);
        add_query_arg(&mut path, &mut sep, "start-time", &start_time);

        Ok(self.0.get(&path).await?.expect_json()?.data)
    }

    /// Return PBS subscription info.
    pub async fn get_subscription(&self) -> Result<proxmox_subscription::SubscriptionInfo, Error> {
        Ok(self
            .0
            .get("/api2/extjs/nodes/localhost/subscription")
            .await?
            .expect_json()?
            .data)
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
