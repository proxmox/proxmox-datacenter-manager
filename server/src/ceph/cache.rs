//! Cached Ceph cluster status, on top of the shared [`crate::api_cache`].
//!
//! `ceph status` is a cluster-level fact - identical no matter which member
//! served it - so it lives in the cache's global namespace keyed by fsid,
//! cached once per cluster rather than per remote. Used to answer the status
//! endpoint within a caller-chosen freshness window and to fill the cheap
//! cluster-list overview (health, reachable mon count) without a live fetch.

use anyhow::Error;
use serde_json::Value;

use crate::api_cache;

fn status_key(fsid: &str) -> String {
    format!("ceph-status-{fsid}")
}

/// The cached `ceph status` for a cluster, if present and younger than
/// `max_age` seconds.
pub async fn cached_status(fsid: &str, max_age: i64) -> Result<Option<Value>, Error> {
    let cache = api_cache::read_global().await?;
    Ok(cache.get_with_max_age(&status_key(fsid), max_age).await?)
}

/// Store a freshly fetched `ceph status` for a cluster.
pub async fn store_status(fsid: &str, status: &Value) -> Result<(), Error> {
    let cache = api_cache::write_global().await?;
    cache.set(&status_key(fsid), status.clone()).await?;
    Ok(())
}

/// Drop the cached status for a cluster (after it is forgotten or its last
/// remote is removed).
pub async fn invalidate(fsid: &str) -> Result<(), Error> {
    let cache = api_cache::write_global().await?;
    cache.remove(&status_key(fsid)).await?;
    Ok(())
}

/// Ceph health string (`HEALTH_OK` / `HEALTH_WARN` / `HEALTH_ERR`) from a
/// `ceph status` object, if present.
pub fn health_from_status(status: &Value) -> Option<String> {
    status
        .get("health")
        .and_then(|h| h.get("status"))
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Number of monitors currently in quorum, from a `ceph status` object.
pub fn quorum_count_from_status(status: &Value) -> i64 {
    status
        .get("quorum_names")
        .and_then(|v| v.as_array())
        .map(|names| names.len() as i64)
        .unwrap_or(0)
}
