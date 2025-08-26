//! API types for metric collection.

use serde::{Deserialize, Serialize};

use proxmox_schema::api;

#[api]
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
/// Per-remote collection status.
pub struct MetricCollectionStatus {
    /// The remote's name.
    pub remote: String,
    /// Any error that occured during the last collection attempt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Timestamp of last successful collection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_collection: Option<i64>,
}
