//! Read/write the Ceph cluster registry (`ceph-clusters.cfg`).
//!
//! The file holds two section types - `ceph-cluster` and `ceph-member` - which
//! the single-type [`SectionConfigData<T>`] wrapper cannot express, so this is
//! the one place that builds the combined [`SectionConfig`] (both plugins) and
//! owns parse/write. The section schemas come from the `#[api]`-derived
//! `API_SCHEMA` of [`CephCluster`] and [`CephMember`]; the types deliberately
//! carry no `ApiSectionDataEntry` impl, to avoid a second, drifting definition.
//!
//! Lock semantics: cross-process serialisation comes from
//! `/etc/proxmox-datacenter-manager/.ceph-clusters.lck` via
//! [`open_api_lockfile`]; never lock in-process around mutations.
//!
//! Forward-compat policy: the schemas must NOT use
//! `#[serde(deny_unknown_fields)]`, so an older PDM can *read* a newer
//! `ceph-clusters.cfg` without erroring (codified in
//! `ceph_clusters_cfg_parses_unknown_key_cleanly`). Beyond parse tolerance,
//! unknown keys are also *preserved* across a rewrite: [`parse`] stashes any
//! key the typed struct does not model into [`CephClustersConfig::extras`], and
//! [`write`] merges them back into the section before serialising (codified in
//! `roundtrip_preserves_unknown_keys`). This keeps a downgrade / mixed-version
//! edit from silently dropping fields a newer PDM wrote. The typed structs
//! still carry no extras bag - the preservation lives here, in the one layer
//! that owns the raw section values.

use std::collections::HashMap;
use std::sync::OnceLock;

use anyhow::{format_err, Error};
use serde::Deserialize;
use serde_json::{Map, Value};

use proxmox_config_digest::ConfigDigest;
use proxmox_product_config::{open_api_lockfile, replace_config, ApiLockGuard};
use proxmox_schema::ApiType;
use proxmox_section_config::{SectionConfig, SectionConfigData, SectionConfigPlugin};

use pdm_api_types::ceph::{CephCluster, CephMember, CEPH_CLUSTER_ID_SCHEMA};
use pdm_buildcfg::configdir;

pub const CEPH_CLUSTERS_CFG_FILENAME: &str = configdir!("/ceph-clusters.cfg");
pub const CEPH_CLUSTERS_CFG_LOCKFILE: &str = configdir!("/.ceph-clusters.lck");

/// Combined section-config schema for the two `ceph-clusters.cfg` section types.
fn ceph_section_config() -> &'static SectionConfig {
    static CFG: OnceLock<SectionConfig> = OnceLock::new();
    CFG.get_or_init(|| {
        let mut cfg = SectionConfig::new(&CEPH_CLUSTER_ID_SCHEMA);
        cfg.register_plugin(SectionConfigPlugin::new(
            "ceph-cluster".to_string(),
            Some("id".to_string()),
            CephCluster::API_SCHEMA.unwrap_object_schema(),
        ));
        cfg.register_plugin(SectionConfigPlugin::new(
            "ceph-member".to_string(),
            Some("id".to_string()),
            CephMember::API_SCHEMA.unwrap_object_schema(),
        ));
        cfg
    })
}

/// Typed view over `ceph-clusters.cfg`.
///
/// The two HashMaps are keyed by their respective section ids. Lookups
/// from a cluster fsid to its members go via [`Self::members_of`].
#[derive(Clone, Debug, Default)]
pub struct CephClustersConfig {
    pub clusters: HashMap<String, CephCluster>,
    pub members: HashMap<String, CephMember>,
    /// Per-section keys present in the parsed file that no typed field models,
    /// keyed by section id. Re-emitted verbatim on [`write`] so a rewrite by
    /// this (possibly older) PDM does not drop fields a newer PDM wrote. Only
    /// populated by [`parse`]; mutators that drop a section leave its orphaned
    /// entry here, which is harmless since [`write`] only merges extras for
    /// sections it actually emits.
    extras: HashMap<String, Map<String, Value>>,
}

impl CephClustersConfig {
    /// Members whose `cluster` field equals the given cluster id.
    pub fn members_of<'a>(
        &'a self,
        cluster_id: &'a str,
    ) -> impl Iterator<Item = &'a CephMember> + 'a {
        self.members
            .values()
            .filter(move |m| m.cluster == cluster_id)
    }

    /// Drop every member of type `pve` that references the given remote.
    /// Returns `true` if any row was removed.
    ///
    /// Intended to be called from the PVE-remote-delete path (together with
    /// [`Self::drop_clusters_without_members`]) so the registry does not keep
    /// dangling member rows. NOT yet wired into `remove_remote`: that hook
    /// lands with auto-detection, once the registry is actually populated and
    /// the cascade can be tested end-to-end.
    pub fn drop_pve_members_for_remote(&mut self, remote_id: &str) -> bool {
        use pdm_api_types::ceph::CephMemberKind;
        let before = self.members.len();
        self.members.retain(|_, m| {
            !(m.kind == CephMemberKind::Pve && m.remote.as_deref() == Some(remote_id))
        });
        before != self.members.len()
    }

    /// Drop every cluster that no longer has any member rows.
    /// Returns the set of dropped cluster ids.
    pub fn drop_clusters_without_members(&mut self) -> Vec<String> {
        let alive: std::collections::HashSet<&String> =
            self.members.values().map(|m| &m.cluster).collect();
        let dropped: Vec<String> = self
            .clusters
            .keys()
            .filter(|id| !alive.contains(id))
            .cloned()
            .collect();
        for id in &dropped {
            self.clusters.remove(id);
        }
        dropped
    }
}

/// Parse the raw section-config text into the typed view.
pub fn parse(filename: &str, raw: &str) -> Result<CephClustersConfig, Error> {
    let cfg = ceph_section_config().parse(filename, raw)?;
    untyped_to_typed(cfg)
}

fn untyped_to_typed(cfg: SectionConfigData) -> Result<CephClustersConfig, Error> {
    let mut out = CephClustersConfig::default();
    for (id, (section_type, value)) in cfg.sections {
        match section_type.as_str() {
            "ceph-cluster" => {
                let cluster = CephCluster::deserialize(&value)
                    .map_err(|e| format_err!("failed to parse ceph-cluster '{id}': {e}"))?;
                if let Some(extras) = unknown_keys(&value, &cluster) {
                    out.extras.insert(id.clone(), extras);
                }
                out.clusters.insert(id, cluster);
            }
            "ceph-member" => {
                let member = CephMember::deserialize(&value)
                    .map_err(|e| format_err!("failed to parse ceph-member '{id}': {e}"))?;
                if let Some(extras) = unknown_keys(&value, &member) {
                    out.extras.insert(id.clone(), extras);
                }
                out.members.insert(id, member);
            }
            other => {
                return Err(format_err!(
                    "unexpected section type '{other}' in {CEPH_CLUSTERS_CFG_FILENAME}"
                ));
            }
        }
    }
    Ok(out)
}

/// The keys present in the raw section `value` that the typed struct does not
/// model, found by diffing the raw object against the re-serialised typed value
/// (a known field always round-trips to the same kebab-case key, so what is left
/// is exactly the unknown keys). Returns `None` when there is nothing to keep.
fn unknown_keys<T: serde::Serialize>(raw: &Value, typed: &T) -> Option<Map<String, Value>> {
    let Value::Object(raw) = raw else {
        return None;
    };
    let typed = serde_json::to_value(typed).ok()?;
    let known = typed.as_object()?;
    let extras: Map<String, Value> = raw
        .iter()
        .filter(|(key, _)| !known.contains_key(*key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    (!extras.is_empty()).then_some(extras)
}

/// Render the typed view back to section-config text.
pub fn write(filename: &str, data: &CephClustersConfig) -> Result<String, Error> {
    let mut raw = SectionConfigData::default();
    // Emit clusters then members, each in sorted id order, and record that
    // order explicitly: the section store is a HashMap, so without this the
    // written file would reorder on every save, churning the digest and
    // triggering spurious rewrites / optimistic-locking conflicts.
    let mut cluster_ids: Vec<&String> = data.clusters.keys().collect();
    cluster_ids.sort();
    for id in cluster_ids {
        let value = data.merge_extras(id, serde_json::to_value(&data.clusters[id])?);
        raw.sections
            .insert(id.clone(), ("ceph-cluster".to_string(), value));
        raw.record_order(id);
    }
    let mut member_ids: Vec<&String> = data.members.keys().collect();
    member_ids.sort();
    for id in member_ids {
        let value = data.merge_extras(id, serde_json::to_value(&data.members[id])?);
        raw.sections
            .insert(id.clone(), ("ceph-member".to_string(), value));
        raw.record_order(id);
    }
    ceph_section_config().write(filename, &raw)
}

impl CephClustersConfig {
    /// Fold the preserved unknown keys for section `id` back into its serialised
    /// `value`. Typed keys always win, so a field that became known in a newer
    /// schema is not shadowed by a stale extra.
    fn merge_extras(&self, id: &str, mut value: Value) -> Value {
        if let (Some(extras), Value::Object(obj)) = (self.extras.get(id), &mut value) {
            for (key, extra) in extras {
                obj.entry(key.clone()).or_insert_with(|| extra.clone());
            }
        }
        value
    }
}

pub fn lock_config() -> Result<ApiLockGuard, Error> {
    open_api_lockfile(CEPH_CLUSTERS_CFG_LOCKFILE, None, true)
}

pub fn config() -> Result<(CephClustersConfig, ConfigDigest), Error> {
    let raw =
        proxmox_sys::fs::file_read_optional_string(CEPH_CLUSTERS_CFG_FILENAME)?.unwrap_or_default();
    let digest = openssl::sha::sha256(raw.as_bytes());
    let data = parse(CEPH_CLUSTERS_CFG_FILENAME, &raw)?;
    Ok((data, digest.into()))
}

pub fn save_config(data: &CephClustersConfig) -> Result<(), Error> {
    let raw = write(CEPH_CLUSTERS_CFG_FILENAME, data)?;
    replace_config(CEPH_CLUSTERS_CFG_FILENAME, raw.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdm_api_types::ceph::CephMemberKind;

    const FSID: &str = "4b4f7f8e-1234-5678-abcd-ef0123456789";

    #[test]
    fn ceph_clusters_cfg_parses_unknown_key_cleanly() {
        // Phase 1b smoke test. Codifies the forward-compat parser policy:
        // older PDM must accept a cfg file written by a newer PDM with extra
        // keys it does not understand. If anyone ever adds
        // #[serde(deny_unknown_fields)] this test breaks.
        let raw = format!(
            r#"
ceph-cluster: {FSID}
    display-name prod-east
    public-network 10.10.0.0/16
    future-field-we-dont-know-yet hello
"#
        );
        let parsed = parse("ceph-clusters.cfg", &raw).expect("forward-compat parse must succeed");
        let cluster = parsed
            .clusters
            .get(FSID)
            .expect("cluster must be in parsed output");
        assert_eq!(cluster.display_name.as_deref(), Some("prod-east"));
        assert_eq!(cluster.public_network.as_deref(), Some("10.10.0.0/16"),);
    }

    fn make_cfg() -> CephClustersConfig {
        let mut cfg = CephClustersConfig::default();
        cfg.clusters.insert(
            FSID.to_string(),
            CephCluster {
                id: FSID.to_string(),
                display_name: Some("prod-east".to_string()),
                public_network: None,
                cluster_network: None,
                stretch_mode: None,
                tiebreaker_mon: None,
                state: None,
                last_seen_missing: None,
                forgotten: None,
            },
        );
        let m1_id = format!("{FSID}-pve-host1");
        cfg.members.insert(
            m1_id.clone(),
            CephMember {
                id: m1_id,
                cluster: FSID.to_string(),
                kind: CephMemberKind::Pve,
                remote: Some("pve-cluster-east".to_string()),
                node: Some("host1".to_string()),
                address: None,
                fingerprint: None,
                authid: None,
                site: None,
            },
        );
        let m2_id = format!("{FSID}-pve-host2");
        cfg.members.insert(
            m2_id.clone(),
            CephMember {
                id: m2_id,
                cluster: FSID.to_string(),
                kind: CephMemberKind::Pve,
                remote: Some("pve-cluster-east".to_string()),
                node: Some("host2".to_string()),
                address: None,
                fingerprint: None,
                authid: None,
                site: None,
            },
        );
        cfg
    }

    #[test]
    fn delete_pve_remote_drops_orphaned_ceph_members() {
        // Phase 1b smoke test. Removing a PVE remote cascades through the
        // hook to drop ceph-member rows referencing it; if that leaves a
        // cluster with no members the cluster is dropped too.
        let mut cfg = make_cfg();
        assert!(cfg.drop_pve_members_for_remote("pve-cluster-east"));
        assert!(cfg.members.is_empty());
        let dropped = cfg.drop_clusters_without_members();
        assert_eq!(dropped, vec![FSID.to_string()]);
        assert!(cfg.clusters.is_empty());
    }

    #[test]
    fn roundtrip_preserves_unknown_keys() {
        // Forward-compat: a key written by a newer PDM that this code does not
        // model must survive a parse -> write cycle, so a downgrade edit does
        // not silently drop it. Asserts the writer re-emits it (the section
        // value carries it and the schema allows additional properties).
        let raw = format!(
            r#"
ceph-cluster: {FSID}
    display-name prod-east
    future-cluster-knob enabled

ceph-member: {FSID}-pve-host1
    cluster {FSID}
    kind pve
    remote pve-cluster-east
    node host1
    future-member-knob 42
"#
        );
        let parsed = parse("ceph-clusters.cfg", &raw).expect("parse must succeed");
        // The unknown keys are stashed, not modeled on the typed structs.
        assert_eq!(parsed.extras.len(), 2);

        let written = write("ceph-clusters.cfg", &parsed).expect("write must succeed");
        assert!(
            written.contains("future-cluster-knob enabled"),
            "unknown cluster key dropped on rewrite:\n{written}"
        );
        assert!(
            written.contains("future-member-knob 42"),
            "unknown member key dropped on rewrite:\n{written}"
        );

        // And the typed fields still round-trip alongside the preserved extras.
        let reparsed = parse("ceph-clusters.cfg", &written).expect("reparse must succeed");
        assert_eq!(
            reparsed.clusters.get(FSID).unwrap().display_name.as_deref(),
            Some("prod-east")
        );
        assert_eq!(reparsed.extras.len(), 2);
    }

    #[test]
    fn roundtrip_preserves_cluster_and_members() {
        let cfg = make_cfg();
        let raw = write("ceph-clusters.cfg", &cfg).expect("write must succeed");
        let parsed = parse("ceph-clusters.cfg", &raw).expect("parse must succeed");
        assert_eq!(parsed.clusters.len(), cfg.clusters.len());
        assert_eq!(parsed.members.len(), cfg.members.len());
    }
}
