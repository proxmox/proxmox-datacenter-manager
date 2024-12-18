//! Read/write remotes configuration
//!
//! Make sure to call [`init`] to inject a concrete `RemoteConfig` instance
//! before calling the [`lock_config`], [`config`] or [`save_config`] functions.

use std::sync::OnceLock;

use anyhow::Error;

use proxmox_config_digest::ConfigDigest;
use proxmox_product_config::{open_api_lockfile, replace_config, ApiLockGuard};
use proxmox_section_config::typed::{ApiSectionDataEntry, SectionConfigData};

use pdm_api_types::remotes::Remote;

use pdm_buildcfg::configdir;

pub const REMOTES_CFG_FILENAME: &str = configdir!("/remotes.cfg");
pub const REMOTES_CFG_LOCKFILE: &str = configdir!("/.remotes.lock");

static INSTANCE: OnceLock<Box<dyn RemoteConfig + Send + Sync>> = OnceLock::new();

fn instance() -> &'static (dyn RemoteConfig + Send + Sync) {
    // Not initializing the remote config instance is
    // entirely in our responsibility and not something we can recover from,
    // so it should be okay to panic in this case.
    INSTANCE
        .get()
        .expect("remote config not initialized")
        .as_ref()
}

/// Lock the remotes config
///
/// Will panic if the the remote config instance has not been set before.
pub fn lock_config() -> Result<ApiLockGuard, Error> {
    instance().lock_config()
}

/// Return contents of the remotes config
///
/// Will panic if the the remote config instance has not been set before.
pub fn config() -> Result<(SectionConfigData<Remote>, ConfigDigest), Error> {
    instance().config()
}

/// Replace the currently persisted remotes config
///
/// Will panic if the the remote config instance has not been set before.
pub fn save_config(config: &SectionConfigData<Remote>) -> Result<(), Error> {
    instance().save_config(config)
}

pub trait RemoteConfig {
    /// Return contents of the remotes config
    fn config(&self) -> Result<(SectionConfigData<Remote>, ConfigDigest), Error>;
    /// Lock the remotes config
    fn lock_config(&self) -> Result<ApiLockGuard, Error>;
    /// Replace the currently persisted remotes config
    fn save_config(&self, remotes: &SectionConfigData<Remote>) -> Result<(), Error>;
}

/// Default, production implementation for reading/writing the `remotes.cfg`
/// config file.
pub struct DefaultRemoteConfig;

impl RemoteConfig for DefaultRemoteConfig {
    fn lock_config(&self) -> Result<ApiLockGuard, Error> {
        open_api_lockfile(REMOTES_CFG_LOCKFILE, None, true)
    }

    fn config(&self) -> Result<(SectionConfigData<Remote>, ConfigDigest), Error> {
        let content =
            proxmox_sys::fs::file_read_optional_string(REMOTES_CFG_FILENAME)?.unwrap_or_default();

        let digest = openssl::sha::sha256(content.as_bytes());
        let data = Remote::parse_section_config(REMOTES_CFG_FILENAME, &content)?;
        Ok((data, digest.into()))
    }

    fn save_config(&self, config: &SectionConfigData<Remote>) -> Result<(), Error> {
        let raw = Remote::write_section_config(REMOTES_CFG_FILENAME, config)?;
        replace_config(REMOTES_CFG_FILENAME, raw.as_bytes())
    }
}

/// Initialize the [`RemoteConfig`] instance.
///
/// Will panic if the instance has already been set.
pub fn init(instance: Box<dyn RemoteConfig + Send + Sync>) {
    if INSTANCE.set(instance).is_err() {
        panic!("remote config instance already set");
    }
}
