//! Basic API types used by most of the PDM code.

use std::error::Error as StdError;
use std::fmt;

use anyhow::{bail, Error};
use serde::{Deserialize, Serialize};

use proxmox_schema::api_types::SAFE_ID_REGEX;
use proxmox_schema::{
    api, const_regex, ApiStringFormat, ApiType, ArraySchema, IntegerSchema, ReturnType, Schema,
    StringSchema, Updater,
};
use proxmox_time::parse_daily_duration;

pub mod common_regex;

mod acl;
pub use acl::*;

pub use proxmox_auth_api::types as userid;
pub use proxmox_auth_api::types::{Authid, Userid};
pub use proxmox_auth_api::types::{Realm, RealmRef};
pub use proxmox_auth_api::types::{Tokenname, TokennameRef};
pub use proxmox_auth_api::types::{Username, UsernameRef};
pub use proxmox_auth_api::types::{
    PROXMOX_GROUP_ID_SCHEMA, PROXMOX_TOKEN_ID_SCHEMA, PROXMOX_TOKEN_NAME_SCHEMA,
};

#[macro_use]
mod user;
pub use user::*;

pub use proxmox_schema::upid::*;

mod openid;
pub use openid::*;

pub mod remotes;

#[rustfmt::skip]
#[macro_use]
mod local_macros {
    macro_rules! DNS_LABEL { () => (r"(?:[a-zA-Z0-9](?:[a-zA-Z0-9\-]*[a-zA-Z0-9])?)") }
    macro_rules! DNS_NAME { () => (concat!(r"(?:(?:", DNS_LABEL!() , r"\.)*", DNS_LABEL!(), ")")) }
    macro_rules! CIDR_V4_REGEX_STR { () => (concat!(r"(?:", IPV4RE!(), r"/\d{1,2})$")) }
    macro_rules! CIDR_V6_REGEX_STR { () => (concat!(r"(?:", IPV6RE!(), r"/\d{1,3})$")) }
    macro_rules! DNS_ALIAS_LABEL { () => (r"(?:[a-zA-Z0-9_](?:[a-zA-Z0-9\-]*[a-zA-Z0-9])?)") }
    macro_rules! DNS_ALIAS_NAME {
        () => (concat!(r"(?:(?:", DNS_ALIAS_LABEL!() , r"\.)*", DNS_ALIAS_LABEL!(), ")"))
    }
    macro_rules! PORT_REGEX_STR { () => (r"(?:[0-9]{1,4}|[1-5][0-9]{4}|6[0-4][0-9]{3}|65[0-4][0-9]{2}|655[0-2][0-9]|6553[0-5])") }
}

const_regex! {
    pub IP_V4_REGEX = concat!(r"^", IPV4RE!(), r"$");
    pub IP_V6_REGEX = concat!(r"^", IPV6RE!(), r"$");
    pub IP_REGEX = concat!(r"^", IPRE!(), r"$");
    pub CIDR_V4_REGEX =  concat!(r"^", CIDR_V4_REGEX_STR!(), r"$");
    pub CIDR_V6_REGEX =  concat!(r"^", CIDR_V6_REGEX_STR!(), r"$");
    pub CIDR_REGEX =  concat!(r"^(?:", CIDR_V4_REGEX_STR!(), "|",  CIDR_V6_REGEX_STR!(), r")$");
    pub HOSTNAME_REGEX = r"^(?:[a-zA-Z0-9](?:[a-zA-Z0-9\-]*[a-zA-Z0-9])?)$";
    pub DNS_NAME_REGEX =  concat!(r"^", DNS_NAME!(), r"$");
    pub DNS_ALIAS_REGEX =  concat!(r"^", DNS_ALIAS_NAME!(), r"$");
    pub DNS_NAME_OR_IP_REGEX = concat!(r"^(?:", DNS_NAME!(), "|",  IPRE!(), r")$");
    pub HOST_PORT_REGEX = concat!(r"^(?:", DNS_NAME!(), "|", IPRE_BRACKET!(), "):", PORT_REGEX_STR!() ,"$");
    pub HTTP_URL_REGEX = concat!(r"^https?://(?:(?:(?:", DNS_NAME!(), "|", IPRE_BRACKET!(), ")(?::", PORT_REGEX_STR!() ,")?)|", IPV6RE!(),")(?:/[^\x00-\x1F\x7F]*)?$");

    pub SHA256_HEX_REGEX = r"^[a-f0-9]{64}$"; // fixme: define in common_regex ?

    pub PASSWORD_REGEX = r"^[[:^cntrl:]]*$"; // everything but control characters

    pub UUID_REGEX = r"^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$";

    pub SYSTEMD_DATETIME_REGEX = r"^\d{4}-\d{2}-\d{2}( \d{2}:\d{2}(:\d{2})?)?$"; //  fixme: define in common_regex ?

    pub FINGERPRINT_SHA256_REGEX = r"^(?:[0-9a-fA-F][0-9a-fA-F])(?::[0-9a-fA-F][0-9a-fA-F]){31}$";

    // just a rough check - dummy acceptor is used before persisting
    pub OPENSSL_CIPHERS_REGEX = r"^[0-9A-Za-z_:, +!\-@=.]+$";

    pub SINGLE_LINE_COMMENT_REGEX = r"^[[:^cntrl:]]*$";

    pub MULTI_LINE_COMMENT_REGEX = r"(?m)^([[:^cntrl:]]*)$";

    pub BLOCKDEVICE_NAME_REGEX = r"^(:?(:?h|s|x?v)d[a-z]+)|(:?nvme\d+n\d+)$";
    pub SUBSCRIPTION_KEY_REGEX = concat!(r"^pbs(?:[cbsp])-[0-9a-f]{10}$");
}

pub const IP_V4_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&IP_V4_REGEX);
pub const IP_V6_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&IP_V6_REGEX);
pub const IP_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&IP_REGEX);
pub const CIDR_V4_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&CIDR_V4_REGEX);
pub const CIDR_V6_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&CIDR_V6_REGEX);
pub const CIDR_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&CIDR_REGEX);
pub const PVE_CONFIG_DIGEST_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&SHA256_HEX_REGEX);
pub const PASSWORD_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&PASSWORD_REGEX);
pub const UUID_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&UUID_REGEX);
pub const BLOCKDEVICE_NAME_FORMAT: ApiStringFormat =
    ApiStringFormat::Pattern(&BLOCKDEVICE_NAME_REGEX);
pub const SUBSCRIPTION_KEY_FORMAT: ApiStringFormat =
    ApiStringFormat::Pattern(&SUBSCRIPTION_KEY_REGEX);
pub const SYSTEMD_DATETIME_FORMAT: ApiStringFormat =
    ApiStringFormat::Pattern(&SYSTEMD_DATETIME_REGEX);
pub const HOSTNAME_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&HOSTNAME_REGEX);
pub const OPENSSL_CIPHERS_TLS_FORMAT: ApiStringFormat =
    ApiStringFormat::Pattern(&OPENSSL_CIPHERS_REGEX);
pub const HOST_PORT_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&HOST_PORT_REGEX);
pub const HTTP_URL_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&HTTP_URL_REGEX);

pub const DNS_ALIAS_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&DNS_ALIAS_REGEX);

pub const DAILY_DURATION_FORMAT: ApiStringFormat =
    ApiStringFormat::VerifyFn(|s| parse_daily_duration(s).map(drop));

pub const SEARCH_DOMAIN_SCHEMA: Schema =
    StringSchema::new("Search domain for host-name lookup.").schema();

pub const FIRST_DNS_SERVER_SCHEMA: Schema = StringSchema::new("First name server IP address.")
    .format(&IP_FORMAT)
    .schema();

pub const SECOND_DNS_SERVER_SCHEMA: Schema = StringSchema::new("Second name server IP address.")
    .format(&IP_FORMAT)
    .schema();

pub const THIRD_DNS_SERVER_SCHEMA: Schema = StringSchema::new("Third name server IP address.")
    .format(&IP_FORMAT)
    .schema();

pub const HOSTNAME_SCHEMA: Schema = StringSchema::new("Hostname (as defined in RFC1123).")
    .format(&HOSTNAME_FORMAT)
    .schema();

pub const OPENSSL_CIPHERS_TLS_1_2_SCHEMA: Schema =
    StringSchema::new("OpenSSL cipher list used by the api server for TLS <= 1.2")
        .format(&OPENSSL_CIPHERS_TLS_FORMAT)
        .schema();

pub const OPENSSL_CIPHERS_TLS_1_3_SCHEMA: Schema =
    StringSchema::new("OpenSSL ciphersuites list used by the api server for TLS 1.3")
        .format(&OPENSSL_CIPHERS_TLS_FORMAT)
        .schema();

pub const DNS_NAME_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&DNS_NAME_REGEX);

pub const DNS_NAME_OR_IP_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&DNS_NAME_OR_IP_REGEX);

pub const DNS_NAME_OR_IP_SCHEMA: Schema = StringSchema::new("DNS name or IP address.")
    .format(&DNS_NAME_OR_IP_FORMAT)
    .schema();

pub const HOST_PORT_SCHEMA: Schema =
    StringSchema::new("host:port combination (Host can be DNS name or IP address).")
        .format(&HOST_PORT_FORMAT)
        .schema();

pub const HTTP_URL_SCHEMA: Schema = StringSchema::new("HTTP(s) url with optional port.")
    .format(&HTTP_URL_FORMAT)
    .schema();

pub const NODE_SCHEMA: Schema = StringSchema::new("Node name (or 'localhost')")
    .format(&HOSTNAME_FORMAT)
    .schema();

pub const TIME_ZONE_SCHEMA: Schema = StringSchema::new(
    "Time zone. The file '/usr/share/zoneinfo/zone.tab' contains the list of valid names.",
)
.format(&SINGLE_LINE_COMMENT_FORMAT)
.min_length(2)
.max_length(64)
.schema();

pub const PASSWORD_SCHEMA: Schema = StringSchema::new("Password.")
    .format(&PASSWORD_FORMAT)
    .min_length(1)
    .max_length(1024)
    .schema();

pub const PDM_PASSWORD_SCHEMA: Schema = StringSchema::new("User Password.")
    .format(&PASSWORD_FORMAT)
    .min_length(5)
    .max_length(64)
    .schema();

pub const REALM_ID_SCHEMA: Schema = StringSchema::new("Realm name.")
    .format(&PROXMOX_SAFE_ID_FORMAT)
    .min_length(2)
    .max_length(32)
    .schema();

pub const FINGERPRINT_SHA256_FORMAT: ApiStringFormat =
    ApiStringFormat::Pattern(&FINGERPRINT_SHA256_REGEX);

pub const CERT_FINGERPRINT_SHA256_SCHEMA: Schema =
    StringSchema::new("X509 certificate fingerprint (sha256).")
        .format(&FINGERPRINT_SHA256_FORMAT)
        .schema();

pub const PROXMOX_SAFE_ID_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&SAFE_ID_REGEX);

pub const SINGLE_LINE_COMMENT_FORMAT: ApiStringFormat =
    ApiStringFormat::Pattern(&SINGLE_LINE_COMMENT_REGEX);

pub const SINGLE_LINE_COMMENT_SCHEMA: Schema = StringSchema::new("Comment (single line).")
    .format(&SINGLE_LINE_COMMENT_FORMAT)
    .schema();

pub const MULTI_LINE_COMMENT_FORMAT: ApiStringFormat =
    ApiStringFormat::Pattern(&MULTI_LINE_COMMENT_REGEX);

pub const MULTI_LINE_COMMENT_SCHEMA: Schema = StringSchema::new("Comment (multiple lines).")
    .format(&MULTI_LINE_COMMENT_FORMAT)
    .schema();

pub const SERVICE_ID_SCHEMA: Schema = StringSchema::new("Service ID.").max_length(256).schema();

pub const PROXMOX_CONFIG_DIGEST_SCHEMA: Schema = StringSchema::new(
    "Prevent changes if current configuration file has different \
    SHA256 digest. This can be used to prevent concurrent \
    modifications.",
)
.format(&PVE_CONFIG_DIGEST_FORMAT)
.schema();

pub const VMID_SCHEMA: Schema = IntegerSchema::new("A guest ID").minimum(1).schema();
pub const SNAPSHOT_NAME_SCHEMA: Schema = StringSchema::new("The name of the snapshot")
    .format(&PROXMOX_SAFE_ID_FORMAT)
    .max_length(40)
    .schema();

// Complex type definitions

#[api()]
#[derive(Default, Serialize, Deserialize)]
/// Storage space usage information.
pub struct StorageStatus {
    /// Total space (bytes).
    pub total: u64,
    /// Used space (bytes).
    pub used: u64,
    /// Available space (bytes).
    pub avail: u64,
}

pub const PASSWORD_HINT_SCHEMA: Schema = StringSchema::new("Password hint.")
    .format(&SINGLE_LINE_COMMENT_FORMAT)
    .min_length(1)
    .max_length(64)
    .schema();

#[api()]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
/// Describes a package for which an update is available.
pub struct APTUpdateInfo {
    /// Package name
    pub package: String,
    /// Package title
    pub title: String,
    /// Package architecture
    pub arch: String,
    /// Human readable package description
    pub description: String,
    /// New version to be updated to
    pub version: String,
    /// Old version currently installed
    pub old_version: String,
    /// Package origin
    pub origin: String,
    /// Package priority in human-readable form
    pub priority: String,
    /// Package section
    pub section: String,
    /// URL under which the package's changelog can be retrieved
    pub change_log_url: String,
    /// Custom extra field for additional package information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_info: Option<String>,
}

#[api()]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
/// Node Power command type.
pub enum NodePowerCommand {
    /// Restart the server
    Reboot,
    /// Shutdown the server
    Shutdown,
}

#[api()]
#[derive(Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStateType {
    /// Ok
    OK,
    /// Warning
    Warning,
    /// Error
    Error,
    /// Unknown
    Unknown,
}

#[api(
    properties: {
        upid: { schema: UPID::API_SCHEMA },
    },
)]
#[derive(Serialize, Deserialize)]
/// Task properties.
pub struct TaskListItem {
    pub upid: String,
    /// The node name where the task is running on.
    pub node: String,
    /// The Unix PID
    pub pid: i64,
    /// The task start time (Epoch)
    pub pstart: u64,
    /// The task start time (Epoch)
    pub starttime: i64,
    /// Worker type (arbitrary ASCII string)
    pub worker_type: String,
    /// Worker ID (arbitrary ASCII string)
    pub worker_id: Option<String>,
    /// The authenticated entity who started the task
    pub user: String,
    /// The task end time (Epoch)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endtime: Option<i64>,
    /// Task end status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

pub const NODE_TASKS_LIST_TASKS_RETURN_TYPE: ReturnType = ReturnType {
    optional: false,
    schema: &ArraySchema::new("A list of tasks.", &TaskListItem::API_SCHEMA).schema(),
};

#[api]
#[derive(Deserialize, Serialize, Copy, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
/// type of the realm
pub enum RealmType {
    /// The PAM realm
    Pam,
    /// The PDM realm
    Pdm,
    /// An OpenID Connect realm
    OpenId,
}

#[api(
    properties: {
        realm: {
            schema: REALM_ID_SCHEMA,
        },
        "type": {
            type: RealmType,
        },
        comment: {
            optional: true,
            schema: SINGLE_LINE_COMMENT_SCHEMA,
        },
    },
)]
#[derive(Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
/// Basic Information about a realm
pub struct BasicRealmInfo {
    pub realm: String,
    #[serde(rename = "type")]
    pub ty: RealmType,
    /// True if it is the default realm
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[api]
/// Guest configuration access.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Updater)]
#[serde(rename_all = "kebab-case")]
pub enum ConfigurationState {
    /// The configuration with pending values.
    #[default]
    Pending,

    /// the configuration with active values.
    Active,
}

impl ConfigurationState {
    /// This is how the PVE client uses it.
    pub fn current(self) -> Option<bool> {
        Some(match self {
            ConfigurationState::Active => true,
            ConfigurationState::Pending => false,
        })
    }
}

serde_plain::derive_display_from_serialize!(ConfigurationState);
serde_plain::derive_fromstr_from_deserialize!(
    ConfigurationState,
    |_err| -> BadConfigurationState { BadConfigurationState }
);

#[derive(Debug)]
pub struct BadConfigurationState;

impl StdError for BadConfigurationState {}

impl fmt::Display for BadConfigurationState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("not a valid configuration state")
    }
}

pub const REMOTE_UPID_SCHEMA: Schema = StringSchema::new("A remote UPID")
    .min_length("C!UPID:N:12345678:12345678:12345678:::".len())
    .schema();

#[derive(Clone, Debug)]
pub struct RemoteUpid {
    remote: String,
    /// This is usually a pve upid, but may also be a pbs upid, they have distinct formats.
    pub upid: String,
}

impl RemoteUpid {
    pub fn remote(&self) -> &str {
        &self.remote
    }

    pub fn into_remote(self) -> String {
        self.remote
    }
}

impl ApiType for RemoteUpid {
    const API_SCHEMA: Schema = REMOTE_UPID_SCHEMA;
}

impl TryFrom<(String, String)> for RemoteUpid {
    type Error = Error;

    fn try_from((remote, upid): (String, String)) -> Result<Self, Error> {
        if !SAFE_ID_REGEX.is_match(&remote) {
            bail!("bad remote id in remote upid");
        }
        Ok(Self { remote, upid })
    }
}

impl TryFrom<(String, &str)> for RemoteUpid {
    type Error = Error;

    fn try_from((remote, upid): (String, &str)) -> Result<Self, Error> {
        if !SAFE_ID_REGEX.is_match(&remote) {
            bail!("bad remote id in remote upid");
        }
        Ok(Self {
            remote,
            upid: upid.to_string(),
        })
    }
}

impl TryFrom<(&str, &str)> for RemoteUpid {
    type Error = Error;

    fn try_from((remote, upid): (&str, &str)) -> Result<Self, Error> {
        if !SAFE_ID_REGEX.is_match(remote) {
            bail!("bad remote id in remote upid");
        }
        Ok(Self {
            remote: remote.to_string(),
            upid: upid.to_string(),
        })
    }
}

impl std::str::FromStr for RemoteUpid {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        match s.find('!') {
            None => bail!("missing '!' separator in remote upid"),
            Some(pos) => (&s[..pos], &s[(pos + 1)..]).try_into(),
        }
    }
}

impl fmt::Display for RemoteUpid {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}!{}", self.remote, self.upid)
    }
}

serde_plain::derive_deserialize_from_fromstr!(RemoteUpid, "valid remote upid");
serde_plain::derive_serialize_from_display!(RemoteUpid);
