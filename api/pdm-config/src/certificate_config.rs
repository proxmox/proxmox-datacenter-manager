use std::collections::HashSet;

use anyhow::{bail, Error};
use pdm_api_types::ConfigDigest;
use serde::{Deserialize, Serialize};

use proxmox_schema::{api, ApiStringFormat, ApiType, Updater};

use proxmox_acme_api::{AcmeConfig, AcmeDomain, ACME_DOMAIN_PROPERTY_SCHEMA};
use proxmox_product_config::{open_api_lockfile, replace_config, ApiLockGuard};

use pdm_buildcfg::configdir;

const CONF_FILE: &str = configdir!("/acme/certificate.cfg");
const LOCK_FILE: &str = configdir!("/acme/.certificate.lck");

pub fn lock() -> Result<ApiLockGuard, Error> {
    open_api_lockfile(LOCK_FILE, None, true)
}

/// Read the certificate configuration (account + domains).
pub fn config() -> Result<(AcmeCertificateConfig, ConfigDigest), Error> {
    let content = proxmox_sys::fs::file_read_optional_string(CONF_FILE)?.unwrap_or_default();
    let digest = ConfigDigest::from_slice(content.as_bytes());
    let data: AcmeCertificateConfig =
        proxmox_simple_config::from_str(&content, &AcmeCertificateConfig::API_SCHEMA)?;
    Ok((data, digest))
}

/// Write the certifgicate configuration, requires the write lock to be held.
pub fn save_config(config: &AcmeCertificateConfig) -> Result<(), Error> {
    config.validate()?;

    let raw = proxmox_simple_config::to_bytes(config, &AcmeCertificateConfig::API_SCHEMA)?;
    replace_config(CONF_FILE, &raw)
}

#[api(
    properties: {
        acme: {
            optional: true,
            type: String,
            format: &ApiStringFormat::PropertyString(&AcmeConfig::API_SCHEMA),
        },
        acmedomain0: {
            schema: ACME_DOMAIN_PROPERTY_SCHEMA,
            optional: true,
        },
        acmedomain1: {
            schema: ACME_DOMAIN_PROPERTY_SCHEMA,
            optional: true,
        },
        acmedomain2: {
            schema: ACME_DOMAIN_PROPERTY_SCHEMA,
            optional: true,
        },
        acmedomain3: {
            schema: ACME_DOMAIN_PROPERTY_SCHEMA,
            optional: true,
        },
        acmedomain4: {
            schema: ACME_DOMAIN_PROPERTY_SCHEMA,
            optional: true,
        },
    },
)]
#[derive(Deserialize, Serialize, Updater)]
#[serde(rename_all = "kebab-case")]
/// Certificate configuration.
pub struct AcmeCertificateConfig {
    /// The acme account to use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acme: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub acmedomain0: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub acmedomain1: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub acmedomain2: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub acmedomain3: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub acmedomain4: Option<String>,
}

impl AcmeCertificateConfig {
    pub fn acme_config(&self) -> Option<Result<AcmeConfig, Error>> {
        self.acme.as_deref().map(|config| -> Result<_, Error> {
            proxmox_simple_config::from_property_string(config, &AcmeConfig::API_SCHEMA)
        })
    }

    pub fn acme_domains(&self) -> AcmeDomainIter {
        AcmeDomainIter::new(self)
    }

    /// Validate the configuration.
    pub fn validate(&self) -> Result<(), Error> {
        let mut domains = HashSet::new();
        for domain in self.acme_domains() {
            let domain = domain?;
            if !domains.insert(domain.domain.to_lowercase()) {
                bail!("duplicate domain '{}' in ACME config", domain.domain);
            }
        }
        Ok(())
    }
}

pub struct AcmeDomainIter<'a> {
    config: &'a AcmeCertificateConfig,
    index: usize,
}

impl<'a> AcmeDomainIter<'a> {
    fn new(config: &'a AcmeCertificateConfig) -> Self {
        Self { config, index: 0 }
    }
}

impl<'a> Iterator for AcmeDomainIter<'a> {
    type Item = Result<AcmeDomain, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let domain = loop {
            let index = self.index;
            self.index += 1;

            let domain = match index {
                0 => self.config.acmedomain0.as_deref(),
                1 => self.config.acmedomain1.as_deref(),
                2 => self.config.acmedomain2.as_deref(),
                3 => self.config.acmedomain3.as_deref(),
                4 => self.config.acmedomain4.as_deref(),
                _ => return None,
            };

            if let Some(domain) = domain {
                break domain;
            }
        };

        Some(proxmox_simple_config::from_property_string(
            domain,
            &AcmeDomain::API_SCHEMA,
        ))
    }
}
