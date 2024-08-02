use std::sync::OnceLock;

use anyhow::Error;

use proxmox_config_digest::ConfigDigest;
use proxmox_product_config::{open_api_lockfile, replace_config, ApiLockGuard};
use proxmox_schema::ApiType;
use proxmox_section_config::{SectionConfig, SectionConfigPlugin};
use proxmox_section_config_typed::SectionConfigData;

use pdm_api_types::remotes::Remote;

pub const REMOTES_CFG_FILENAME: &str = "/etc/proxmox-datacenter-manager/remotes.cfg";
pub const REMOTES_CFG_LOCKFILE: &str = "/etc/proxmox-datacenter-manager/.remotes.lock";

static CONFIG: OnceLock<SectionConfig> = OnceLock::new();

fn section_config() -> &'static SectionConfig {
    CONFIG.get_or_init(|| {
        let mut this =
            SectionConfig::new(&pdm_api_types::remotes::REMOTE_ID_SCHEMA).with_type_key("type");
        this.register_plugin(SectionConfigPlugin::new(
            "pve".to_string(),
            Some("id".to_string()),
            Remote::API_SCHEMA.unwrap_object_schema(),
        ));
        this
    })
}

/// Get exclusive lock
pub fn lock_config() -> Result<ApiLockGuard, Error> {
    open_api_lockfile(REMOTES_CFG_LOCKFILE, None, true)
}

pub fn config() -> Result<(SectionConfigData<Remote>, ConfigDigest), Error> {
    let content =
        proxmox_sys::fs::file_read_optional_string(REMOTES_CFG_FILENAME)?.unwrap_or_default();

    let digest = openssl::sha::sha256(content.as_bytes());
    let data = section_config()
        .parse(REMOTES_CFG_FILENAME, &content)?
        .try_into()?;
    Ok((data, digest.into()))
}

pub fn save_config(config: &SectionConfigData<Remote>) -> Result<(), Error> {
    let raw = section_config().write(REMOTES_CFG_FILENAME, &config.try_into()?)?;
    replace_config(REMOTES_CFG_FILENAME, raw.as_bytes())
}
