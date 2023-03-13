use anyhow::Error;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use proxmox_schema::ApiType;
use proxmox_section_config::{SectionConfig, SectionConfigPlugin};

use pdm_api_types::{PveRemote, REMOTE_ID_SCHEMA};

use crate::section_config::{ApiSectionDataEntry, SectionConfigData};
use crate::{open_api_lockfile, replace_api_config, ApiLockGuard, ConfigDigest};

pub const REMOTES_CFG_FILENAME: &str = "/etc/proxmox-datacenter-manager/remotes.cfg";
pub const REMOTES_CFG_LOCKFILE: &str = "/etc/proxmox-datacenter-manager/.remotes.lock";

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
    replace_api_config(REMOTES_CFG_FILENAME, raw.as_bytes())
}

static CONFIG: Lazy<SectionConfig> = Lazy::new(|| {
    let mut this = SectionConfig::new(&REMOTE_ID_SCHEMA);
    this.register_plugin(SectionConfigPlugin::new(
        "pve".to_string(),
        Some("id".to_string()),
        PveRemote::API_SCHEMA.unwrap_object_schema(),
    ));
    this
});

/// In the future we may also have PMG or PBS nodes.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Remote {
    Pve(PveRemote),
}

impl From<PveRemote> for Remote {
    fn from(pve: PveRemote) -> Self {
        Remote::Pve(pve)
    }
}

// To be derived via a macro from the enum.
impl ApiSectionDataEntry for Remote {
    const INTERNALLY_TAGGED: Option<&'static str> = Some("type");

    fn section_config() -> &'static SectionConfig {
        &CONFIG
    }

    fn section_type(&self) -> &'static str {
        match self {
            Remote::Pve(_) => "pve",
        }
    }
}

impl Remote {
    pub fn id(&self) -> &str {
        match self {
            Self::Pve(r) => &r.id,
        }
    }
}
