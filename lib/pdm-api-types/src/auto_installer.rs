//! API types used for the auto-installation configuration.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt::Debug};

use proxmox_auth_api::types::Userid;
use proxmox_installer_types::{
    answer::SUBSCRIPTION_KEY_SCHEMA, post_hook::PostHookInfo, SystemInfo,
};
use proxmox_network_types::ip_address::{api_types::IpAddr, Cidr};
use proxmox_schema::{
    api,
    api_types::{CERT_FINGERPRINT_SHA256_SCHEMA, COMMENT_SCHEMA, HTTP_URL_SCHEMA, UUID_FORMAT},
    const_regex, ApiStringFormat, Schema, StringSchema, Updater,
};
use proxmox_uuid::Uuid;

use crate::{BLOCKDEVICE_NAME_SCHEMA, EMAIL_SCHEMA, PROXMOX_TOKEN_NAME_SCHEMA};

/// Re-export for convenience, as these types are used within [`PreparedInstallationConfig`].
pub use proxmox_installer_types::answer;

pub const INSTALLATION_UUID_SCHEMA: Schema = StringSchema::new("UUID of a installation.")
    .format(&UUID_FORMAT)
    .schema();

#[api]
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
/// Current status of an installation.
pub enum InstallationStatus {
    /// An appropriate answer file was found and sent to the machine. Post-hook was unavailable,
    /// so no further status is received.
    AnswerSent,
    /// Found no matching answer configuration and no default was set.
    NoAnswerFound,
    /// The installation is currently underway.
    InProgress,
    /// The installation was finished successfully.
    Finished,
}

#[api(
    properties: {
        uuid: {
            schema: INSTALLATION_UUID_SCHEMA,
        },
        "received-at": {
            minimum: 0,
        },
    },
)]
#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
/// A installation received from some proxmox-auto-installer instance.
pub struct Installation {
    /// Unique ID of this installation.
    pub uuid: Uuid,
    /// Time the installation request was received (Unix Epoch).
    pub received_at: i64,
    /// Current status of this installation.
    pub status: InstallationStatus,
    /// System information about the machine to be provisioned.
    pub info: SystemInfo,
    /// Answer that was sent to the target machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_id: Option<String>,
    /// Post-installation notification hook data, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_hook_data: Option<PostHookInfo>,
    /// Per-installation secret used to authenticate the post-hook callback.
    /// Persisted on disk only; stripped before being returned over the API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_hook_token: Option<String>,
}

#[api]
#[derive(Debug, Default, Copy, Clone, PartialEq, Deserialize, Serialize, Updater)]
#[serde(rename_all = "lowercase")]
/// How to select the target installations disks.
pub enum DiskSelectionMode {
    #[default]
    /// Use the fixed list of disks.
    Fixed,
    /// Dynamically determine target disks based on udev filters.
    Filter,
}

serde_plain::derive_fromstr_from_deserialize!(DiskSelectionMode);

const_regex! {
    /// Regex for matching the key of udev filters.
    pub UDEV_FILTER_KEY_REGEX = r"^(?:[A-Z_]+)$";

    /// Regex for matching the name of template counters.
    pub TEMPLATE_COUNTER_NAME_REGEX = r"^(?:[A-Za-z_][A-Za-z0-9_]*)$";
}

pub const PREPARED_INSTALL_CONFIG_ID_SCHEMA: proxmox_schema::Schema =
    StringSchema::new("ID of prepared configuration for automated installations.")
        .min_length(3)
        .max_length(64)
        .schema();

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
            type: Object,
            properties: {},
            additional_properties: true,
            optional: true,
        },
        country: {
            format: &ApiStringFormat::Pattern(&answer::COUNTRY_CODE_REGEX),
            min_length: 2,
            max_length: 2,
        },
        mailto: {
            schema: EMAIL_SCHEMA,
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
            type: Object,
            properties: {},
            additional_properties: true,
            optional: true,
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
            type: Object,
            properties: {},
            additional_properties: true,
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
            type: Object,
            properties: {},
            additional_properties: true,
            optional: true,
        },
        "subscription-key": {
            optional: true,
            schema: SUBSCRIPTION_KEY_SCHEMA,
        },
    },
)]
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Updater)]
#[serde(rename_all = "kebab-case")]
/// Configuration describing an automated installation.
///
/// Certain fields support simple templating via [MiniJinja]. Currently, following fields will
/// resolve MiniJinja expressions upon instantiation of an answer:
///
/// * `fqdn`
/// * `mailto`
/// * `cidr`
/// * `gateway`
/// * `dns`
///
/// [MiniJinja]: https://docs.rs/minijinja/
pub struct PreparedInstallationConfig {
    #[updater(skip)]
    pub id: String,

    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// List of token IDs that are authorized to retrieve this answer.
    pub authorized_tokens: Vec<String>,

    /// Whether this is the default answer. There can only ever be one default answer.
    /// `target_filter` below is ignored if this is `true`.
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
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
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub target_filter: BTreeMap<String, String>,

    // Keys from [`answer::GlobalOptions`], adapted to better fit the API and model of the UI.
    /// Country to use for apt mirrors.
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub country: String,
    /// FQDN to set for the installed system. Only used if `use_dhcp_fqdn` is true.
    ///
    /// Supports templating via MiniJinja.
    /// The [`proxmox_network_types::fqdn::Fqdn`] type cannot be used here
    /// because of that, as curly brackets are not valid in hostnames.
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub fqdn: String,
    /// Whether to use the FQDN from the DHCP lease or the user-provided one.
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub use_dhcp_fqdn: bool,
    /// Keyboard layout to set.
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub keyboard: answer::KeyboardLayout,
    /// Mail address for `root@pam`.
    ///
    /// Supports templating via MiniJinja.
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub mailto: String,
    /// Timezone to set on the new system.
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub timezone: String,
    /// Pre-hashed password to set for the `root` PAM account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub root_password_hashed: Option<String>,
    /// Whether to reboot the machine if an error occurred during the
    /// installation.
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub reboot_on_error: bool,
    /// Action to take after the installation completed successfully.
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub reboot_mode: answer::RebootMode,
    /// Newline-separated list of public SSH keys to set up for the `root` PAM account.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub root_ssh_keys: Vec<String>,

    // Keys from [`answer::NetworkConfig`], adapted to better fit the API and model of the UI.
    /// Whether to use the network configuration from the DHCP lease or not.
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub use_dhcp_network: bool,
    /// IP address and netmask if not using DHCP.
    ///
    /// Supports templating via MiniJinja.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub cidr: Option<Cidr>,
    /// Gateway if not using DHCP.
    ///
    /// Supports templating via MiniJinja.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub gateway: Option<IpAddr>,
    /// DNS server address if not using DHCP.
    ///
    /// Supports templating via MiniJinja.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub dns: Option<IpAddr>,

    /// Filter for network devices, to select a specific management interface.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub netdev_filter: BTreeMap<String, String>,
    /// Whether to enable network interface name pinning.
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub netif_name_pinning_enabled: bool,

    /// Root filesystem options.
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub filesystem: answer::FilesystemOptions,

    /// Whether to use the fixed disk list or select disks dynamically by udev filters.
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub disk_mode: DiskSelectionMode,
    /// List of raw disk identifiers to use for the root filesystem.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub disk_list: Vec<String>,
    /// Filter against udev properties to select the disks for the installation,
    /// to allow dynamic selection of disks.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub disk_filter: BTreeMap<String, String>,
    /// Whether it is enough that any filter matches on a disk or all given
    /// filters must match to select a disk. Only used if `disk_list` is unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub disk_filter_match: Option<answer::FilterMatch>,

    /// Post installations hook base URL, i.e. host PDM is reachable as from
    /// the target machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub post_hook_base_url: Option<String>,
    /// Post hook certificate fingerprint, if needed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub post_hook_cert_fp: Option<String>,

    /// Key-value pairs of (auto-incrementing) counters.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub template_counters: BTreeMap<String, i32>,

    /// Optional Proxmox subscription key to apply to the installed system on
    /// first boot. Forwarded as-is into the rendered answer's `[global]`
    /// section; the installer stages it for the proxmox-first-boot
    /// subscription activator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[updater(serde(default, skip_serializing_if = "Option::is_none"))]
    pub subscription_key: Option<String>,
}

#[api]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
/// Deletable property names for [`PreparedInstallationConfig`]
pub enum DeletablePreparedInstallationConfigProperty {
    /// Delete all target filters
    TargetFilter,
    /// Delete all udev property filters for the management network device
    NetdevFilter,
    /// Delete all udev property filters for disks
    DiskFilter,
    /// Delete all `root` user public ssh keys.
    RootSshKeys,
    /// Delete the post-installation notification base url.
    PostHookBaseUrl,
    /// Delete the post-installation notification certificate fingerprint.
    PostHookCertFp,
    /// Delete all templating counters.
    TemplateCounters,
    /// Drop the configured subscription key.
    SubscriptionKey,
}

serde_plain::derive_display_from_serialize!(DeletablePreparedInstallationConfigProperty);
serde_plain::derive_fromstr_from_deserialize!(DeletablePreparedInstallationConfigProperty);

#[api]
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
/// Result when creating a new installation configuration.
pub struct PreparedInstallationConfigCreateResult {
    /// Final prepared installation configuration.
    pub config: PreparedInstallationConfig,
    /// Newly created token to access this prepared configuration, if no authorized tokens were
    /// specified on creation.
    pub token: Option<AnswerTokenCreateResult>,
}

pub type PreparedInstallationConfigUpdateResult = PreparedInstallationConfigCreateResult;

#[api(
     properties: {
        id: {
            schema: PROXMOX_TOKEN_NAME_SCHEMA,
        },
        "created-by": {
            type: String,
        },
        comment: {
            optional: true,
            schema: COMMENT_SCHEMA,
        },
        enabled: {
            type: bool,
            optional: true,
            default: true,
        },
        "expire-at": {
            type: Integer,
            optional: true,
            minimum: 0,
            description: "Token expiration date (seconds since epoch). '0' means no expiration date.",
        },
     }
 )]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Updater)]
#[serde(rename_all = "kebab-case")]
/// A token for authenticating requests from the automated installer.
pub struct AnswerToken {
    #[updater(skip)]
    /// Name of the auth token
    pub id: String,
    #[updater(skip)]
    /// Name of the user that created it
    pub created_by: Userid,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    /// Optional comment
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    /// Whether this token is enabled
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    /// Expiration time of this token, if any
    pub expire_at: Option<i64>,
}

impl AnswerToken {
    pub fn is_active(&self) -> bool {
        self.enabled.unwrap_or(true)
            && self
                .expire_at
                .map(|exp| exp == 0 || exp > proxmox_time::epoch_i64())
                .unwrap_or(true)
    }
}

#[api]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
/// Deletable property names for [`AnswerToken`].
pub enum DeletableAnswerTokenProperty {
    /// Delete the comment
    Comment,
    /// Delete the expiration date
    ExpireAt,
}

serde_plain::derive_display_from_serialize!(DeletableAnswerTokenProperty);
serde_plain::derive_fromstr_from_deserialize!(DeletableAnswerTokenProperty);

#[api]
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
/// Result when creating a new token.
pub struct AnswerTokenCreateResult {
    /// Information about the token.
    pub token: AnswerToken,
    /// The secret value itself.
    pub secret: String,
}

#[api]
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
/// Result when updating a new token.
pub struct AnswerTokenUpdateResult {
    /// Information about the token.
    pub token: AnswerToken,
    /// The secret, if a new one was requested to be generated.
    pub secret: Option<String>,
}
