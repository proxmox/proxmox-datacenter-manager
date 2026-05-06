//! Implements configuration for the auto-installer integration.

use anyhow::{anyhow, bail, Result};
use std::collections::HashMap;

use pdm_api_types::{
    auto_installer::{AnswerToken, Installation},
    ConfigDigest,
};
use proxmox_product_config::{open_api_lockfile, replace_config, ApiLockGuard};
use proxmox_section_config::typed::{ApiSectionDataEntry, SectionConfigData};

use crate::auto_install::types::AnswerTokenWrapper;

pub const CONFIG_PATH: &str = pdm_buildcfg::configdir!("/autoinst");

const PREPARED_CONF_FILE: &str = pdm_buildcfg::configdir!("/autoinst/prepared.cfg");
const PREPARED_LOCK_FILE: &str = pdm_buildcfg::configdir!("/autoinst/.prepared.lock");

const TOKENS_CONF_FILE: &str = pdm_buildcfg::configdir!("/autoinst/tokens.cfg");
const TOKENS_SHADOW_FILE: &str = pdm_buildcfg::configdir!("/autoinst/tokens.shadow");
const TOKENS_LOCK_FILE: &str = pdm_buildcfg::configdir!("/autoinst/.tokens.lock");

const INSTALLATIONS_STATE_FILE: &str = pdm_buildcfg::statedir!("/automated-installations.json");
const INSTALLATIONS_LOCK_FILE: &str = pdm_buildcfg::statedir!("/.automated-installations.lock");

pub mod types {
    use serde::{Deserialize, Serialize};
    use std::{collections::BTreeMap, fmt::Debug};

    use pdm_api_types::{
        auto_installer::{
            answer, AnswerToken, DiskSelectionMode, PreparedInstallationConfig,
            PREPARED_INSTALL_CONFIG_ID_SCHEMA,
        },
        BLOCKDEVICE_NAME_SCHEMA, CERT_FINGERPRINT_SHA256_SCHEMA, HTTP_URL_SCHEMA,
        PROXMOX_TOKEN_NAME_SCHEMA, SINGLE_LINE_COMMENT_FORMAT,
    };
    use proxmox_network_types::{api_types::IpAddr, Cidr};
    use proxmox_schema::{api, ApiStringFormat, ApiType, PropertyString};

    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    /// API wrapper for a [`BTreeMap`].
    pub struct BTreeMapWrapper<T>(BTreeMap<String, T>);

    impl<T> std::ops::Deref for BTreeMapWrapper<T> {
        type Target = BTreeMap<String, T>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<T> std::ops::DerefMut for BTreeMapWrapper<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl<T: for<'de> Deserialize<'de> + Serialize> ApiType for BTreeMapWrapper<T> {
        const API_SCHEMA: proxmox_schema::Schema =
            proxmox_schema::ObjectSchema::new("Map of key-value pairs", &[])
                .additional_properties(true)
                .schema();
    }

    #[api(
        "id-property": "id",
        "id-schema": {
            type: String,
            description: "ID of prepared configuration for automated installations.",
            min_length: 3,
            max_length: 64
        }
    )]
    #[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "kebab-case", tag = "type")]
    /// Wrapper type for using [`PreparedInstallationConfig`] with
    /// [`proxmox_schema::typed::SectionConfigData`].
    pub enum PreparedInstallationSectionConfigWrapper {
        /// Prepared Installation Configuration.
        PreparedConfig(PreparedInstallationSectionConfig),
    }

    #[api(
        properties: {
            id: {
                schema: PREPARED_INSTALL_CONFIG_ID_SCHEMA,
            },
            "authorized-tokens": {
                type: Array,
                optional: true,
                items: {
                    schema: PROXMOX_TOKEN_NAME_SCHEMA,
                },
            },
            "is-default": {
                optional: true,
            },
            "target-filter": {
                type: String,
                optional: true,
            },
            country: {
                format: &ApiStringFormat::Pattern(&answer::COUNTRY_CODE_REGEX),
                min_length: 2,
                max_length: 2,
            },
            mailto: {
                min_length: 2,
                max_length: 256,
                format: &SINGLE_LINE_COMMENT_FORMAT,
            },
            "root-ssh-keys": {
                type: Array,
                optional: true,
                items: {
                    type: String,
                    description: "SSH public key.",
                },
            },
            "netdev-filter": {
                type: String,
                optional: true,
            },
            filesystem: {
                type: String,
            },
            "disk-mode": {
                type: String,
            },
            "disk-list": {
                type: Array,
                optional: true,
                items: {
                    schema: BLOCKDEVICE_NAME_SCHEMA,
                },
            },
            "disk-filter": {
                type: String,
                optional: true,
            },
            "post-hook-base-url": {
                schema: HTTP_URL_SCHEMA,
                optional: true,
            },
            "post-hook-cert-fp": {
                schema: CERT_FINGERPRINT_SHA256_SCHEMA,
                optional: true,
            },
            "template-counters": {
                type: String,
                optional: true,
            },
        },
    )]
    #[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "kebab-case")]
    /// Configuration describing an automated installation.
    ///
    /// Certain fields support simple templating via [Handlebars]. Currently, following fields will
    /// resolve handlebars expression upon instantiation of an answer:
    ///
    /// * `fqdn`
    /// * `mailto`
    /// * `cidr`
    /// * `gateway`
    /// * `dns
    ///
    /// [Handlebars]: https://handlebarsjs.com/guide/
    pub struct PreparedInstallationSectionConfig {
        pub id: String,

        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        /// List of token IDs that are authoried to retrieve this answer.
        pub authorized_tokens: Vec<String>,

        /// Whether this is the default answer. There can only ever be one default answer.
        /// `target_filter` below is ignored if this is `true`.
        pub is_default: bool,

        // Target filters
        /// Map of filters for matching against a property in [`answer::fetch::AnswerFetchData`].
        /// The keys are JSON Pointers as per [RFC6901], the values globs as accepted
        /// by the [glob] crate.
        ///
        /// Used to check this configuration against incoming automated installation requests. If this
        /// is unset, it will match any installation not matched "narrower" by other prepared
        /// configurations, thus being the default.
        ///
        /// [RFC6901] https://datatracker.ietf.org/doc/html/rfc6901
        /// [glob crate] https://docs.rs/glob/
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        pub target_filter: PropertyString<BTreeMapWrapper<String>>,

        // Keys from [`answer::GlobalOptions`], adapted to better fit the API and model of the UI.
        /// Country to use for apt mirrors.
        pub country: String,
        /// FQDN to set for the installed system. Only used if `use_dhcp_fqdn` is true.
        ///
        /// Supports templating via Handlebars.
        /// The [`proxmox_network_types::fqdn::Fqdn`] type cannot be used here
        /// because of that, as curly brackets are not valid in hostnames.
        pub fqdn: String,
        /// Whether to use the FQDN from the DHCP lease or the user-provided one.
        pub use_dhcp_fqdn: bool,
        /// Keyboard layout to set.
        pub keyboard: answer::KeyboardLayout,
        /// Mail address for `root@pam`.
        ///
        /// Supports templating via Handlebars.
        pub mailto: String,
        /// Timezone to set on the new system.
        pub timezone: String,
        /// Pre-hashed password to set for the `root` PAM account.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub root_password_hashed: Option<String>,
        /// Whether to reboot the machine if an error occurred during the
        /// installation.
        pub reboot_on_error: bool,
        /// Action to take after the installation completed successfully.
        pub reboot_mode: answer::RebootMode,
        /// Newline-separated list of public SSH keys to set up for the `root` PAM account.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub root_ssh_keys: Vec<String>,

        // Keys from [`answer::NetworkConfig`], adapted to better fit the API and model of the UI.
        /// Whether to use the network configuration from the DHCP lease or not.
        pub use_dhcp_network: bool,
        /// IP address and netmask if not using DHCP.
        ///
        /// Supports templating via Handlebars.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub cidr: Option<Cidr>,
        /// Gateway if not using DHCP.
        ///
        /// Supports templating via Handlebars.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub gateway: Option<IpAddr>,
        /// DNS server address if not using DHCP.
        ///
        /// Supports templating via Handlebars.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub dns: Option<IpAddr>,
        /// Filter for network devices, to select a specific management interface.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        pub netdev_filter: PropertyString<BTreeMapWrapper<String>>,
        /// Whether to enable network interface name pinning.
        pub netif_name_pinning_enabled: bool,

        /// Root filesystem options.
        pub filesystem: PropertyString<answer::FilesystemOptions>,

        /// Whether to use the fixed disk list or select disks dynamically by udev filters.
        pub disk_mode: DiskSelectionMode,
        /// List of raw disk identifiers to use for the root filesystem.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub disk_list: Vec<String>,
        /// Filter against udev properties to select the disks for the installation,
        /// to allow dynamic selection of disks.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        pub disk_filter: PropertyString<BTreeMapWrapper<String>>,
        /// Whether it is enough that any filter matches on a disk or all given
        /// filters must match to select a disk. Only used if `disk_list` is unset.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub disk_filter_match: Option<answer::FilterMatch>,

        /// Post installations hook base URL, i.e. host PDM is reachable as from
        /// the target machine.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub post_hook_base_url: Option<String>,
        /// Post hook certificate fingerprint, if needed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub post_hook_cert_fp: Option<String>,

        /// Key-value pairs of (auto-incrementing) counters.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        pub template_counters: PropertyString<BTreeMapWrapper<i32>>,
    }

    impl TryFrom<PreparedInstallationConfig> for PreparedInstallationSectionConfig {
        type Error = anyhow::Error;

        fn try_from(conf: PreparedInstallationConfig) -> Result<Self, Self::Error> {
            Ok(Self {
                id: conf.id,
                authorized_tokens: conf.authorized_tokens,
                // target filter
                is_default: conf.is_default,
                target_filter: PropertyString::new(BTreeMapWrapper(conf.target_filter)),
                // global options
                country: conf.country,
                fqdn: conf.fqdn,
                use_dhcp_fqdn: conf.use_dhcp_fqdn,
                keyboard: conf.keyboard,
                mailto: conf.mailto,
                timezone: conf.timezone,
                root_password_hashed: conf.root_password_hashed,
                reboot_on_error: conf.reboot_on_error,
                reboot_mode: conf.reboot_mode,
                root_ssh_keys: conf.root_ssh_keys,
                // network options
                use_dhcp_network: conf.use_dhcp_network,
                cidr: conf.cidr,
                gateway: conf.gateway,
                dns: conf.dns,
                netdev_filter: PropertyString::new(BTreeMapWrapper(conf.netdev_filter)),
                netif_name_pinning_enabled: conf.netif_name_pinning_enabled,
                // disk options
                filesystem: PropertyString::new(conf.filesystem),
                disk_mode: conf.disk_mode,
                disk_list: conf.disk_list,
                disk_filter: PropertyString::new(BTreeMapWrapper(conf.disk_filter)),
                disk_filter_match: conf.disk_filter_match,
                // post hook
                post_hook_base_url: conf.post_hook_base_url,
                post_hook_cert_fp: conf.post_hook_cert_fp,
                // templating
                template_counters: PropertyString::new(BTreeMapWrapper(conf.template_counters)),
            })
        }
    }

    impl TryFrom<PreparedInstallationConfig> for PreparedInstallationSectionConfigWrapper {
        type Error = anyhow::Error;

        fn try_from(conf: PreparedInstallationConfig) -> Result<Self, Self::Error> {
            Ok(Self::PreparedConfig(conf.try_into()?))
        }
    }

    impl TryInto<PreparedInstallationConfig> for PreparedInstallationSectionConfig {
        type Error = anyhow::Error;

        fn try_into(self) -> Result<PreparedInstallationConfig, Self::Error> {
            Ok(PreparedInstallationConfig {
                id: self.id,
                authorized_tokens: self.authorized_tokens,
                // target filter
                is_default: self.is_default,
                target_filter: self.target_filter.into_inner().0,
                // global options
                country: self.country,
                fqdn: self.fqdn,
                use_dhcp_fqdn: self.use_dhcp_fqdn,
                keyboard: self.keyboard,
                mailto: self.mailto,
                timezone: self.timezone,
                root_password_hashed: self.root_password_hashed,
                reboot_on_error: self.reboot_on_error,
                reboot_mode: self.reboot_mode,
                root_ssh_keys: self.root_ssh_keys,
                // network options
                use_dhcp_network: self.use_dhcp_network,
                cidr: self.cidr,
                gateway: self.gateway,
                dns: self.dns,
                netdev_filter: self.netdev_filter.into_inner().0,
                netif_name_pinning_enabled: self.netif_name_pinning_enabled,
                // disk options
                filesystem: self.filesystem.into_inner(),
                disk_mode: self.disk_mode,
                disk_list: self.disk_list,
                disk_filter: self.disk_filter.into_inner().0,
                disk_filter_match: self.disk_filter_match,
                // post hook
                post_hook_base_url: self.post_hook_base_url,
                post_hook_cert_fp: self.post_hook_cert_fp,
                // templating
                template_counters: self.template_counters.into_inner().0,
            })
        }
    }

    impl TryInto<PreparedInstallationConfig> for PreparedInstallationSectionConfigWrapper {
        type Error = anyhow::Error;

        fn try_into(self) -> Result<PreparedInstallationConfig, Self::Error> {
            let PreparedInstallationSectionConfigWrapper::PreparedConfig(conf) = self;
            conf.try_into()
        }
    }

    #[api(
        "id-property": "id",
        "id-schema": {
            type: String,
            description: "Access token name.",
            min_length: 3,
            max_length: 64,
        },
    )]
    #[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
    #[serde(rename_all = "kebab-case", tag = "type")]
    /// Access token for authenticating against the /answer endpoint.
    pub enum AnswerTokenWrapper {
        /// Simple token with a secret, like an API token.
        Token(AnswerToken),
    }

    impl From<AnswerTokenWrapper> for AnswerToken {
        fn from(value: AnswerTokenWrapper) -> Self {
            let AnswerTokenWrapper::Token(token) = value;
            token
        }
    }

    impl From<AnswerToken> for AnswerTokenWrapper {
        fn from(value: AnswerToken) -> Self {
            AnswerTokenWrapper::Token(value)
        }
    }
}

pub fn installations_read_lock() -> Result<ApiLockGuard> {
    open_api_lockfile(INSTALLATIONS_LOCK_FILE, None, false)
}

pub fn installations_write_lock() -> Result<ApiLockGuard> {
    open_api_lockfile(INSTALLATIONS_LOCK_FILE, None, true)
}

pub fn read_installations() -> Result<(Vec<Installation>, ConfigDigest)> {
    let content: serde_json::Value = serde_json::from_str(
        &proxmox_sys::fs::file_read_optional_string(INSTALLATIONS_STATE_FILE)?
            .unwrap_or_else(|| "[]".to_owned()),
    )?;

    let digest = proxmox_serde::json::to_canonical_json(&content).map(ConfigDigest::from_slice)?;
    let data = serde_json::from_value(content)?;

    Ok((data, digest))
}

/// Write lock must be already held.
pub fn save_installations(config: &[Installation]) -> Result<()> {
    let raw = serde_json::to_string(&config)?;
    replace_config(INSTALLATIONS_STATE_FILE, raw.as_bytes())
}

pub fn prepared_answers_read_lock() -> Result<ApiLockGuard> {
    open_api_lockfile(PREPARED_LOCK_FILE, None, false)
}

pub fn prepared_answers_write_lock() -> Result<ApiLockGuard> {
    open_api_lockfile(PREPARED_LOCK_FILE, None, true)
}

pub fn read_prepared_answers() -> Result<(
    SectionConfigData<types::PreparedInstallationSectionConfigWrapper>,
    ConfigDigest,
)> {
    let content =
        proxmox_sys::fs::file_read_optional_string(PREPARED_CONF_FILE)?.unwrap_or_default();

    let digest = ConfigDigest::from_slice(content.as_bytes());
    let data = types::PreparedInstallationSectionConfigWrapper::parse_section_config(
        PREPARED_CONF_FILE,
        &content,
    )?;

    Ok((data, digest))
}

/// Write lock must be already held.
pub fn save_prepared_answers(
    config: &SectionConfigData<types::PreparedInstallationSectionConfigWrapper>,
) -> Result<()> {
    let raw = types::PreparedInstallationSectionConfigWrapper::write_section_config(
        PREPARED_CONF_FILE,
        config,
    )?;
    replace_config(PREPARED_CONF_FILE, raw.as_bytes())
}

pub fn tokens_read_lock() -> Result<ApiLockGuard> {
    open_api_lockfile(TOKENS_LOCK_FILE, None, false)
}

pub fn tokens_write_lock() -> Result<ApiLockGuard> {
    open_api_lockfile(TOKENS_LOCK_FILE, None, true)
}

pub fn read_tokens() -> Result<(SectionConfigData<types::AnswerTokenWrapper>, ConfigDigest)> {
    let content = proxmox_sys::fs::file_read_optional_string(TOKENS_CONF_FILE)?.unwrap_or_default();

    let digest = ConfigDigest::from_slice(content.as_bytes());
    let data = types::AnswerTokenWrapper::parse_section_config(TOKENS_CONF_FILE, &content)?;

    Ok((data, digest))
}

fn write_tokens(data: SectionConfigData<AnswerTokenWrapper>) -> Result<()> {
    let raw = AnswerTokenWrapper::write_section_config(TOKENS_CONF_FILE, &data)?;
    replace_config(TOKENS_CONF_FILE, raw.as_bytes())
}

/// Write lock must be already held.
pub fn add_token(token: &AnswerToken, value: &str) -> Result<()> {
    let (mut tokens, _) = read_tokens()?;

    if tokens.contains_key(&token.id) {
        bail!("token already exists");
    }

    tokens.insert(token.id.clone(), token.clone().into());
    write_tokens(tokens)?;

    let mut shadow = read_token_shadow()?;
    let hashed = proxmox_sys::crypt::encrypt_pw(value)?;
    shadow.insert(token.id.clone(), hashed);
    write_token_shadow(shadow)
}

/// Write lock must be already held.
pub fn update_token(token: &AnswerToken) -> Result<()> {
    let (mut tokens, _) = read_tokens()?;

    if !tokens.contains_key(&token.id) {
        bail!("unknown token: {}", token.id);
    }

    tokens.insert(token.id.clone(), token.clone().into());
    write_tokens(tokens)?;
    Ok(())
}

/// Write lock must be already held.
pub fn update_token_shadow(id: &str, secret: &str) -> Result<()> {
    let mut shadow = read_token_shadow()?;
    if !shadow.contains_key(id) {
        bail!("unknown token: {id}");
    }

    let hashed = proxmox_sys::crypt::encrypt_pw(secret)?;
    shadow.insert(id.to_owned(), hashed);
    write_token_shadow(shadow)
}

/// Write lock must be already held.
pub fn delete_token(id: &str) -> Result<()> {
    let (mut tokens, _) = read_tokens()?;

    if !tokens.contains_key(id) {
        bail!("unknown token: {id}");
    }

    tokens.remove(id);
    write_tokens(tokens)?;

    let mut shadow = read_token_shadow()?;
    shadow.remove(id);
    write_token_shadow(shadow)
}

/// At least read lock must be held.
pub fn verify_secret(id: &str, secret: &str) -> Result<()> {
    read_token_shadow()?
        .get(id)
        .and_then(|hashed| proxmox_sys::crypt::verify_crypt_pw(secret, hashed).ok())
        .ok_or_else(|| anyhow!("invalid access secret"))
}

fn read_token_shadow() -> Result<HashMap<String, String>> {
    Ok(serde_json::from_str(
        &proxmox_sys::fs::file_read_optional_string(TOKENS_SHADOW_FILE)?
            .unwrap_or_else(|| "{}".to_owned()),
    )?)
}

/// Write lock must be already held.
fn write_token_shadow(data: HashMap<String, String>) -> Result<()> {
    let raw = serde_json::to_string(&data)?;
    replace_config(TOKENS_SHADOW_FILE, raw.as_bytes())
}
