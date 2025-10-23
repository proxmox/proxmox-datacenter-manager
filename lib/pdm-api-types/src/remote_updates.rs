use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Serialize};

use proxmox_schema::{api, ApiType, ObjectSchema};

use crate::remotes::RemoteType;

#[api]
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
/// Update summary for all remotes.
pub struct UpdateSummary {
    /// Map containing the update summary each remote.
    pub remotes: RemoteUpdateSummaryWrapper,
}

// This is a hack to allow actual 'maps' (mapping remote name to per-remote data)
// within the realms of our API macro.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RemoteUpdateSummaryWrapper(HashMap<String, RemoteUpdateSummary>);

impl ApiType for RemoteUpdateSummaryWrapper {
    const API_SCHEMA: proxmox_schema::Schema =
        ObjectSchema::new("Map of per-remote update summaries", &[])
            .additional_properties(true)
            .schema();
}

impl Deref for RemoteUpdateSummaryWrapper {
    type Target = HashMap<String, RemoteUpdateSummary>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for RemoteUpdateSummaryWrapper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[api]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
/// Update summary for a single remote.
pub struct RemoteUpdateSummary {
    /// Map containing the update summary for each node of this remote.
    pub nodes: NodeUpdateSummaryWrapper,
    pub remote_type: RemoteType,
    pub status: RemoteUpdateStatus,
}

#[api]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// Status for the entire remote.
pub enum RemoteUpdateStatus {
    /// Successfully polled remote.
    Success,
    /// Remote could not be polled.
    Error,
    /// Remote has not been polled yet.
    Unknown,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct NodeUpdateSummaryWrapper(HashMap<String, NodeUpdateSummary>);

impl ApiType for NodeUpdateSummaryWrapper {
    const API_SCHEMA: proxmox_schema::Schema =
        ObjectSchema::new("Map of per-node update summaries", &[])
            .additional_properties(true)
            .schema();
}

impl Deref for NodeUpdateSummaryWrapper {
    type Target = HashMap<String, NodeUpdateSummary>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NodeUpdateSummaryWrapper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[api]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// Status for the entire remote.
pub enum NodeUpdateStatus {
    /// Successfully polled node.
    Success,
    /// Node could not be polled.
    Error,
}

#[api]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// Per-node update summary.
pub struct NodeUpdateSummary {
    /// Number of available updates.
    pub number_of_updates: u32,
    /// Unix timestamp of the last refresh.
    pub last_refresh: i64,
    /// Status
    pub status: NodeUpdateStatus,
    /// Status message (e.g. error message)
    pub status_message: Option<String>,
}
