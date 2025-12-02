//! Read/write remotes configuration
//!
//! Make sure to call [`init`] to inject a concrete `RemoteConfig` instance
//! before calling the [`lock_config`], [`config`] or [`save_config`] functions.

use std::{collections::HashSet, sync::OnceLock};

use anyhow::{bail, Error};

use proxmox_config_digest::ConfigDigest;
use proxmox_product_config::{open_api_lockfile, replace_config, ApiLockGuard};
use proxmox_section_config::typed::{ApiSectionDataEntry, SectionConfigData};

use pdm_api_types::remotes::{Remote, RemoteShadow};

use pdm_buildcfg::configdir;

pub const REMOTES_CFG_FILENAME: &str = configdir!("/remotes.cfg");
const REMOTES_SHADOW_FILENAME: &str = configdir!("/remotes.shadow");
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

pub fn get_secret_token(remote: &Remote) -> Result<String, Error> {
    instance().get_secret_token(remote)
}

/// Replace the currently persisted remotes config
///
/// Will panic if the the remote config instance has not been set before.
pub fn save_config(config: SectionConfigData<Remote>) -> Result<(), Error> {
    instance().save_config(config)
}

pub trait RemoteConfig {
    /// Return contents of the remotes config
    fn config(&self) -> Result<(SectionConfigData<Remote>, ConfigDigest), Error>;
    /// Return contents of the remotes shadow config
    fn get_secret_token(&self, remote: &Remote) -> Result<String, Error>;
    /// Lock the remotes config
    fn lock_config(&self) -> Result<ApiLockGuard, Error>;
    /// Replace the currently persisted remotes config
    fn save_config(&self, remotes: SectionConfigData<Remote>) -> Result<(), Error>;
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

    fn save_config(&self, mut config: SectionConfigData<Remote>) -> Result<(), Error> {
        let shadow_content = proxmox_sys::fs::file_read_optional_string(REMOTES_SHADOW_FILENAME)?
            .unwrap_or_default();

        let mut shadow_config =
            RemoteShadow::parse_section_config(REMOTES_SHADOW_FILENAME, &shadow_content)?;

        // collect valid remotes
        let mut remote_ids = HashSet::new();

        // collect any remotes which are not yet shadowed
        let new_shadow_entries = config.iter().fold(Vec::new(), |mut entries, (id, remote)| {
            if remote.token != "-" {
                entries.push(RemoteShadow {
                    ty: remote.ty,
                    id: remote.id.clone(),
                    token: remote.token.clone(),
                });
            }
            remote_ids.insert(id.to_string());
            entries
        });

        // remove leftover shadow entries
        let shadow_ids = shadow_config.keys().cloned().collect::<Vec<String>>();
        for id in shadow_ids.iter() {
            if !remote_ids.contains(id) {
                shadow_config.remove(id);
            }
        }

        // add new shadow entries
        for entry in new_shadow_entries.into_iter() {
            if let Some(remote) = config.get_mut(&entry.id) {
                remote.token = "-".to_string();
            }
            shadow_config.insert(entry.id.clone(), entry);
        }

        // write out shadow config
        let raw_shadow =
            RemoteShadow::write_section_config(REMOTES_SHADOW_FILENAME, &shadow_config)?;
        replace_config(REMOTES_SHADOW_FILENAME, raw_shadow.as_bytes())?;

        // write out remotes.cfg *only after shadow config has been written*
        let raw = Remote::write_section_config(REMOTES_CFG_FILENAME, &config)?;
        replace_config(REMOTES_CFG_FILENAME, raw.as_bytes())
    }

    fn get_secret_token(&self, remote: &Remote) -> Result<String, Error> {
        // not yet rewritten into shadow config
        if remote.token != "-" {
            return Ok(remote.token.clone());
        }

        let shadow_content = proxmox_sys::fs::file_read_optional_string(REMOTES_SHADOW_FILENAME)?
            .unwrap_or_default();

        let shadow_config =
            RemoteShadow::parse_section_config(REMOTES_SHADOW_FILENAME, &shadow_content)?;

        if let Some(shadow_entry) = shadow_config.get(&remote.id) {
            Ok(shadow_entry.token.clone())
        } else {
            bail!("No shadow entry found for remote {id}", id = remote.id);
        }
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
