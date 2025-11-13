use anyhow::Error;

use proxmox_section_config::typed::{ApiSectionDataEntry, SectionConfigData};

use pdm_api_types::views::ViewConfigEntry;

use pdm_buildcfg::configdir;

const VIEW_CFG_FILENAME: &str = configdir!("/views.cfg");

/// Get the `views.cfg` config file contents.
pub fn config() -> Result<SectionConfigData<ViewConfigEntry>, Error> {
    let content =
        proxmox_sys::fs::file_read_optional_string(VIEW_CFG_FILENAME)?.unwrap_or_default();

    ViewConfigEntry::parse_section_config(VIEW_CFG_FILENAME, &content)
}
