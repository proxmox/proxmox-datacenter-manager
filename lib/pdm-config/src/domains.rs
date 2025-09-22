use std::collections::HashMap;
use std::sync::LazyLock;

use anyhow::Error;

use proxmox_ldap::types::{AdRealmConfig, LdapRealmConfig};
use proxmox_schema::{ApiType, Schema};
use proxmox_section_config::{SectionConfig, SectionConfigData, SectionConfigPlugin};

use pdm_api_types::{ConfigDigest, OpenIdRealmConfig, REALM_ID_SCHEMA};
use proxmox_product_config::{open_api_lockfile, replace_privileged_config, ApiLockGuard};

use pdm_buildcfg::configdir;

pub static CONFIG: LazyLock<SectionConfig> = LazyLock::new(init);

fn init() -> SectionConfig {
    let obj_schema = match OpenIdRealmConfig::API_SCHEMA {
        Schema::Object(ref obj_schema) => obj_schema,
        _ => unreachable!(),
    };

    let plugin = SectionConfigPlugin::new(
        "openid".to_string(),
        Some(String::from("realm")),
        obj_schema,
    );
    let mut config = SectionConfig::new(&REALM_ID_SCHEMA);
    config.register_plugin(plugin);

    let ldap_plugin = SectionConfigPlugin::new(
        "ldap".to_string(),
        Some("realm".to_string()),
        LdapRealmConfig::API_SCHEMA.unwrap_object_schema(),
    );
    config.register_plugin(ldap_plugin);

    let ad_plugin = SectionConfigPlugin::new(
        "ad".to_string(),
        Some("realm".to_string()),
        AdRealmConfig::API_SCHEMA.unwrap_object_schema(),
    );
    config.register_plugin(ad_plugin);

    config
}

pub const DOMAINS_CFG_FILENAME: &str = configdir!("/access/domains.cfg");
pub const DOMAINS_CFG_LOCKFILE: &str = configdir!("/access/.domains.lock");

/// Get exclusive lock
pub fn lock_config() -> Result<ApiLockGuard, Error> {
    open_api_lockfile(DOMAINS_CFG_LOCKFILE, None, true)
}

pub fn config() -> Result<(SectionConfigData, ConfigDigest), Error> {
    let content =
        proxmox_sys::fs::file_read_optional_string(DOMAINS_CFG_FILENAME)?.unwrap_or_default();
    let digest = ConfigDigest::from_slice(content.as_bytes());
    let data = CONFIG.parse(DOMAINS_CFG_FILENAME, &content)?;
    Ok((data, digest))
}

pub fn save_config(config: &SectionConfigData) -> Result<(), Error> {
    let raw = CONFIG.write(DOMAINS_CFG_FILENAME, config)?;
    replace_privileged_config(DOMAINS_CFG_FILENAME, raw.as_bytes())
}

// shell completion helper
pub fn complete_realm_name(_arg: &str, _param: &HashMap<String, String>) -> Vec<String> {
    match config() {
        Ok((data, _digest)) => data.sections.keys().map(|id| id.to_string()).collect(),
        Err(_) => Vec::new(),
    }
}

pub fn complete_openid_realm_name(_arg: &str, _param: &HashMap<String, String>) -> Vec<String> {
    match config() {
        Ok((data, _digest)) => data
            .sections
            .iter()
            .filter_map(|(id, (t, _))| {
                if t == "openid" {
                    Some(id.to_string())
                } else {
                    None
                }
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}
