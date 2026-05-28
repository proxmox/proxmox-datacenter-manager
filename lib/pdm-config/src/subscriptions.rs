//! Read/write subscription key pool configuration.
//!
//! Call [`init`] to inject a concrete `SubscriptionKeyConfig` instance before using the
//! module-level functions.
//!
//! The shadow-config functions stash signed `SubscriptionInfo` blobs alongside the plain key
//! entries, which is intended as future proofing for a more automated (shop) import without having
//! to adapt the data layer.

use std::sync::OnceLock;

use anyhow::Error;

use proxmox_config_digest::ConfigDigest;
use proxmox_product_config::{
    ApiLockGuard, open_api_lockfile, replace_config, replace_secret_config,
};
use proxmox_section_config::typed::{ApiSectionDataEntry, SectionConfigData};

use pdm_api_types::subscription::{SubscriptionKeyEntry, SubscriptionKeyShadow};
use pdm_buildcfg::configdir;

pub const CONFIG_PATH: &str = configdir!("/subscriptions");
pub const SUBSCRIPTIONS_CFG_FILENAME: &str = configdir!("/subscriptions/keys.cfg");
const SUBSCRIPTIONS_SHADOW_FILENAME: &str = configdir!("/subscriptions/keys.shadow");
pub const SUBSCRIPTIONS_CFG_LOCKFILE: &str = configdir!("/subscriptions/.keys.lock");

static INSTANCE: OnceLock<Box<dyn SubscriptionKeyConfig + Send + Sync>> = OnceLock::new();

fn instance() -> &'static (dyn SubscriptionKeyConfig + Send + Sync) {
    INSTANCE
        .get()
        .expect("subscription key config not initialized")
        .as_ref()
}

pub fn lock_config() -> Result<ApiLockGuard, Error> {
    instance().lock_config()
}

pub fn config() -> Result<(SectionConfigData<SubscriptionKeyEntry>, ConfigDigest), Error> {
    instance().config()
}

pub fn shadow_config() -> Result<SectionConfigData<SubscriptionKeyShadow>, Error> {
    instance().shadow_config()
}

pub fn save_config(
    config: &SectionConfigData<SubscriptionKeyEntry>,
) -> Result<ConfigDigest, Error> {
    instance().save_config(config)
}

pub fn save_shadow(shadow: &SectionConfigData<SubscriptionKeyShadow>) -> Result<(), Error> {
    instance().save_shadow(shadow)
}

pub trait SubscriptionKeyConfig {
    fn config(&self) -> Result<(SectionConfigData<SubscriptionKeyEntry>, ConfigDigest), Error>;
    fn shadow_config(&self) -> Result<SectionConfigData<SubscriptionKeyShadow>, Error>;
    fn lock_config(&self) -> Result<ApiLockGuard, Error>;
    fn save_config(
        &self,
        config: &SectionConfigData<SubscriptionKeyEntry>,
    ) -> Result<ConfigDigest, Error>;
    fn save_shadow(&self, shadow: &SectionConfigData<SubscriptionKeyShadow>) -> Result<(), Error>;
}

pub struct DefaultSubscriptionKeyConfig;

impl SubscriptionKeyConfig for DefaultSubscriptionKeyConfig {
    fn lock_config(&self) -> Result<ApiLockGuard, Error> {
        open_api_lockfile(SUBSCRIPTIONS_CFG_LOCKFILE, None, true)
    }

    fn config(&self) -> Result<(SectionConfigData<SubscriptionKeyEntry>, ConfigDigest), Error> {
        let content = proxmox_sys::fs::file_read_optional_string(SUBSCRIPTIONS_CFG_FILENAME)?
            .unwrap_or_default();

        let digest = openssl::sha::sha256(content.as_bytes());
        let data =
            SubscriptionKeyEntry::parse_section_config(SUBSCRIPTIONS_CFG_FILENAME, &content)?;

        Ok((data, digest.into()))
    }

    fn shadow_config(&self) -> Result<SectionConfigData<SubscriptionKeyShadow>, Error> {
        let content = proxmox_sys::fs::file_read_optional_string(SUBSCRIPTIONS_SHADOW_FILENAME)?
            .unwrap_or_default();
        SubscriptionKeyShadow::parse_section_config(SUBSCRIPTIONS_SHADOW_FILENAME, &content)
    }

    fn save_config(
        &self,
        config: &SectionConfigData<SubscriptionKeyEntry>,
    ) -> Result<ConfigDigest, Error> {
        let raw = SubscriptionKeyEntry::write_section_config(SUBSCRIPTIONS_CFG_FILENAME, config)?;
        let digest: ConfigDigest = openssl::sha::sha256(raw.as_bytes()).into();
        replace_config(SUBSCRIPTIONS_CFG_FILENAME, raw.as_bytes())?;
        Ok(digest)
    }

    fn save_shadow(&self, shadow: &SectionConfigData<SubscriptionKeyShadow>) -> Result<(), Error> {
        let raw =
            SubscriptionKeyShadow::write_section_config(SUBSCRIPTIONS_SHADOW_FILENAME, shadow)?;
        // Signed `SubscriptionInfo` blobs are secrets - mode 0600, priv:priv, so the
        // unprivileged API user cannot read them. The main keys.cfg keeps 0640 since the API
        // process still needs to read the key strings.
        replace_secret_config(SUBSCRIPTIONS_SHADOW_FILENAME, raw.as_bytes())
    }
}

pub fn init(instance: Box<dyn SubscriptionKeyConfig + Send + Sync>) {
    if INSTANCE.set(instance).is_err() {
        panic!("subscription key config instance already set");
    }
}
