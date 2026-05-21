//! Cached Ceph cluster status, on top of the shared [`crate::api_cache`].
//!
//! `ceph status` is a cluster-level fact - identical no matter which member
//! served it - so it lives in the cache's global namespace keyed by fsid,
//! cached once per cluster rather than per remote. Used to answer the status
//! endpoint within a caller-chosen freshness window and to fill the cheap
//! cluster-list overview (health, reachable mon count) without a live fetch.

use anyhow::Error;
use serde_json::Value;

use pdm_api_types::ceph::{CephClusterStatus, CephHealthCheck, CephPgStateGroup};

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

/// Read a JSON number as i64, accepting both signed and unsigned encodings.
fn value_i64(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_u64().map(|u| u as i64))
}

/// Build a typed [`CephClusterStatus`] from a raw `ceph status` object.
///
/// All field access is defensive: missing or renamed fields default rather than
/// failing, so the summary survives Ceph-release shape changes (forward-compat).
pub fn summarize_status(fsid: &str, raw: &Value) -> CephClusterStatus {
    let health = health_from_status(raw).unwrap_or_else(|| "HEALTH_UNKNOWN".to_string());

    let checks = raw
        .get("health")
        .and_then(|h| h.get("checks"))
        .and_then(|c| c.as_object())
        .map(|map| {
            map.iter()
                .map(|(code, detail)| CephHealthCheck {
                    code: code.clone(),
                    severity: detail
                        .get("severity")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    summary: detail
                        .get("summary")
                        .and_then(|s| s.get("message"))
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    let pgmap = raw.get("pgmap");
    let pg = |key: &str| pgmap.and_then(|p| p.get(key)).and_then(value_i64).unwrap_or(0);
    let pg_opt = |key: &str| pgmap.and_then(|p| p.get(key)).and_then(value_i64);
    let pg_f = |key: &str| pgmap.and_then(|p| p.get(key)).and_then(|v| v.as_f64());

    let pgs_by_state = pgmap
        .and_then(|p| p.get("pgs_by_state"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|e| CephPgStateGroup {
                    state_name: e
                        .get("state_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    count: e.get("count").and_then(value_i64).unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default();

    // OSD counts are flat under `osdmap` in current Ceph but nested under
    // `osdmap.osdmap` in older releases; accept either.
    let osdmap = raw.get("osdmap").map(|o| o.get("osdmap").unwrap_or(o));
    let osd = |key: &str| osdmap.and_then(|o| o.get(key)).and_then(value_i64).unwrap_or(0);

    let mons_total = raw
        .get("monmap")
        .and_then(|m| m.get("num_mons"))
        .and_then(value_i64)
        .unwrap_or(0);

    let mgrmap = raw.get("mgrmap");
    let mgr_active = mgrmap
        .and_then(|m| m.get("active_name"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);
    let mgr_standbys = mgrmap
        .and_then(|m| m.get("standbys"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.get("name").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    CephClusterStatus {
        fsid: fsid.to_string(),
        health,
        checks,
        bytes_total: pg("bytes_total"),
        bytes_used: pg("bytes_used"),
        bytes_avail: pg("bytes_avail"),
        num_pools: pg("num_pools"),
        num_pgs: pg("num_pgs"),
        pgs_by_state,
        osds_total: osd("num_osds"),
        osds_up: osd("num_up_osds"),
        osds_in: osd("num_in_osds"),
        mons_total,
        mons_in_quorum: quorum_count_from_status(raw),
        mgr_active,
        mgr_standbys,
        client_read_bytes_sec: pg_opt("read_bytes_sec"),
        client_write_bytes_sec: pg_opt("write_bytes_sec"),
        client_read_ops_sec: pg_opt("read_op_per_sec"),
        client_write_ops_sec: pg_opt("write_op_per_sec"),
        recovery_bytes_sec: pg_opt("recovering_bytes_per_sec"),
        misplaced_ratio: pg_f("misplaced_ratio"),
        degraded_ratio: pg_f("degraded_ratio"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn summarize_extracts_typed_fields() {
        let raw = json!({
            "fsid": "4b4f7f8e-1234-5678-abcd-ef0123456789",
            "health": {
                "status": "HEALTH_WARN",
                "checks": {
                    "OSD_DOWN": {
                        "severity": "HEALTH_WARN",
                        "summary": { "message": "1 osds down" }
                    }
                }
            },
            "monmap": { "num_mons": 3 },
            "quorum_names": ["a", "b", "c"],
            "osdmap": { "num_osds": 6, "num_up_osds": 5, "num_in_osds": 6 },
            "mgrmap": { "active_name": "mgr-a", "standbys": [{ "name": "mgr-b" }] },
            "pgmap": {
                "num_pools": 3,
                "num_pgs": 256,
                "bytes_used": 1000u64,
                "bytes_total": 4000u64,
                "bytes_avail": 3000u64,
                "pgs_by_state": [{ "state_name": "active+clean", "count": 256 }],
                "read_bytes_sec": 1024u64
            }
        });

        let s = summarize_status("4b4f7f8e-1234-5678-abcd-ef0123456789", &raw);
        assert_eq!(s.health, "HEALTH_WARN");
        assert_eq!(s.checks.len(), 1);
        assert_eq!(s.checks[0].code, "OSD_DOWN");
        assert_eq!(s.checks[0].summary, "1 osds down");
        assert_eq!((s.bytes_used, s.bytes_total, s.bytes_avail), (1000, 4000, 3000));
        assert_eq!((s.osds_total, s.osds_up, s.osds_in), (6, 5, 6));
        assert_eq!((s.mons_total, s.mons_in_quorum), (3, 3));
        assert_eq!(s.mgr_active.as_deref(), Some("mgr-a"));
        assert_eq!(s.mgr_standbys, vec!["mgr-b".to_string()]);
        assert_eq!(s.num_pgs, 256);
        assert_eq!(s.pgs_by_state, vec![pdm_api_types::ceph::CephPgStateGroup {
            state_name: "active+clean".to_string(),
            count: 256,
        }]);
        assert_eq!(s.client_read_bytes_sec, Some(1024));
    }

    #[test]
    fn summarize_handles_nested_osdmap_and_missing_fields() {
        let raw = json!({
            "health": { "status": "HEALTH_OK" },
            "osdmap": { "osdmap": { "num_osds": 9, "num_up_osds": 9, "num_in_osds": 9 } }
        });
        let s = summarize_status("fsid", &raw);
        assert_eq!(s.health, "HEALTH_OK");
        assert_eq!((s.osds_total, s.osds_up), (9, 9));
        assert_eq!(s.bytes_total, 0);
        assert!(s.checks.is_empty());
        assert_eq!(s.mons_in_quorum, 0);
    }
}
