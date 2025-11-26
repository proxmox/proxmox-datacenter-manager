use anyhow::Error;

use proxmox_product_config::{open_api_lockfile, replace_config, ApiLockGuard};
use proxmox_section_config::typed::{ApiSectionDataEntry, SectionConfigData};

use pdm_api_types::{views::ViewConfigEntry, ConfigDigest};

use pdm_buildcfg::configdir;

const VIEW_CFG_FILENAME: &str = configdir!("/views.cfg");
const VIEW_FILTER_CFG_LOCKFILE: &str = configdir!("/.views.lock");

/// Get the `views.cfg` config file contents.
pub fn config() -> Result<(SectionConfigData<ViewConfigEntry>, ConfigDigest), Error> {
    let content =
        proxmox_sys::fs::file_read_optional_string(VIEW_CFG_FILENAME)?.unwrap_or_default();

    let digest = openssl::sha::sha256(content.as_bytes());

    let data = ViewConfigEntry::parse_section_config(VIEW_CFG_FILENAME, &content)?;
    Ok((data, digest.into()))
}

/// Get exclusive lock
pub fn lock_config() -> Result<ApiLockGuard, Error> {
    open_api_lockfile(VIEW_FILTER_CFG_LOCKFILE, None, true)
}

pub fn save_config(config: &SectionConfigData<ViewConfigEntry>) -> Result<(), Error> {
    let raw = ViewConfigEntry::write_section_config(VIEW_CFG_FILENAME, config)?;
    replace_config(VIEW_CFG_FILENAME, raw.as_bytes())?;
    Ok(())
}
