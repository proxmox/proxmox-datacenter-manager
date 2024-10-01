use anyhow::Error;

use proxmox_config_digest::ConfigDigest;
use proxmox_product_config::{open_api_lockfile, replace_config, ApiLockGuard};
use proxmox_section_config::typed::{ApiSectionDataEntry, SectionConfigData};

use pdm_api_types::remotes::Remote;

use pdm_buildcfg::configdir;

pub const REMOTES_CFG_FILENAME: &str = configdir!("/remotes.cfg");
pub const REMOTES_CFG_LOCKFILE: &str = configdir!("/.remotes.lock");

/// Get exclusive lock
pub fn lock_config() -> Result<ApiLockGuard, Error> {
    open_api_lockfile(REMOTES_CFG_LOCKFILE, None, true)
}

pub fn config() -> Result<(SectionConfigData<Remote>, ConfigDigest), Error> {
    let content =
        proxmox_sys::fs::file_read_optional_string(REMOTES_CFG_FILENAME)?.unwrap_or_default();

    let digest = openssl::sha::sha256(content.as_bytes());
    let data = Remote::parse_section_config(REMOTES_CFG_FILENAME, &content)?;
    Ok((data, digest.into()))
}

pub fn save_config(config: &SectionConfigData<Remote>) -> Result<(), Error> {
    let raw = Remote::write_section_config(REMOTES_CFG_FILENAME, config)?;
    replace_config(REMOTES_CFG_FILENAME, raw.as_bytes())
}
