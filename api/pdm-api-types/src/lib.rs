//! Basic API types used by most of the PDM code.

use std::fmt;

use anyhow::{bail, Error};
use serde::{Deserialize, Serialize};

use proxmox_schema::api_types::SAFE_ID_REGEX;
use proxmox_schema::{
    api, const_regex, ApiStringFormat, ApiType, ArraySchema, IntegerSchema, ReturnType, Schema,
    StringSchema, Updater,
};
use proxmox_time::parse_daily_duration;

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

pub use proxmox_schema::api_types::SAFE_ID_FORMAT as PROXMOX_SAFE_ID_FORMAT;
pub use proxmox_schema::api_types::SAFE_ID_REGEX as PROXMOX_SAFE_ID_REGEX;
pub use proxmox_schema::api_types::SAFE_ID_REGEX_STR as PROXMOX_SAFE_ID_REGEX_STR;
pub use proxmox_schema::api_types::{
    BLOCKDEVICE_DISK_AND_PARTITION_NAME_REGEX, BLOCKDEVICE_NAME_REGEX,
};
pub use proxmox_schema::api_types::{DNS_ALIAS_REGEX, DNS_NAME_OR_IP_REGEX, DNS_NAME_REGEX};
pub use proxmox_schema::api_types::{FINGERPRINT_SHA256_REGEX, SHA256_HEX_REGEX};
pub use proxmox_schema::api_types::{
    GENERIC_URI_REGEX, HOSTNAME_REGEX, HOST_PORT_REGEX, HTTP_URL_REGEX,
};
pub use proxmox_schema::api_types::{MULTI_LINE_COMMENT_REGEX, SINGLE_LINE_COMMENT_REGEX};
pub use proxmox_schema::api_types::{PASSWORD_REGEX, SYSTEMD_DATETIME_REGEX, UUID_REGEX};

pub use proxmox_schema::api_types::{CIDR_FORMAT, CIDR_REGEX};
pub use proxmox_schema::api_types::{CIDR_V4_FORMAT, CIDR_V4_REGEX};
pub use proxmox_schema::api_types::{CIDR_V6_FORMAT, CIDR_V6_REGEX};
pub use proxmox_schema::api_types::{IPRE_STR, IP_FORMAT, IP_REGEX};
pub use proxmox_schema::api_types::{IPV4RE_STR, IP_V4_FORMAT, IP_V4_REGEX};
pub use proxmox_schema::api_types::{IPV6RE_STR, IP_V6_FORMAT, IP_V6_REGEX};

pub use proxmox_schema::api_types::COMMENT_SCHEMA as SINGLE_LINE_COMMENT_SCHEMA;
pub use proxmox_schema::api_types::HOSTNAME_SCHEMA;
pub use proxmox_schema::api_types::HOST_PORT_SCHEMA;
pub use proxmox_schema::api_types::HTTP_URL_SCHEMA;
pub use proxmox_schema::api_types::MULTI_LINE_COMMENT_SCHEMA;
pub use proxmox_schema::api_types::NODE_SCHEMA;
pub use proxmox_schema::api_types::SINGLE_LINE_COMMENT_FORMAT;
pub use proxmox_schema::api_types::{
    BLOCKDEVICE_DISK_AND_PARTITION_NAME_SCHEMA, BLOCKDEVICE_NAME_SCHEMA,
};
pub use proxmox_schema::api_types::{CERT_FINGERPRINT_SHA256_SCHEMA, FINGERPRINT_SHA256_FORMAT};
pub use proxmox_schema::api_types::{DISK_ARRAY_SCHEMA, DISK_LIST_SCHEMA};
pub use proxmox_schema::api_types::{DNS_ALIAS_FORMAT, DNS_NAME_FORMAT, DNS_NAME_OR_IP_SCHEMA};
pub use proxmox_schema::api_types::{PASSWORD_FORMAT, PASSWORD_SCHEMA};
pub use proxmox_schema::api_types::{SERVICE_ID_SCHEMA, UUID_FORMAT};
pub use proxmox_schema::api_types::{SYSTEMD_DATETIME_FORMAT, TIME_ZONE_SCHEMA};

pub use proxmox_dns_api::SEARCH_DOMAIN_SCHEMA;
pub use proxmox_dns_api::FIRST_DNS_SERVER_SCHEMA;
pub use proxmox_dns_api::SECOND_DNS_SERVER_SCHEMA;
pub use proxmox_dns_api::THIRD_DNS_SERVER_SCHEMA;

pub use proxmox_config_digest::ConfigDigest;
pub use proxmox_config_digest::PROXMOX_CONFIG_DIGEST_SCHEMA;

#[macro_use]
mod user;
pub use user::*;

pub use proxmox_schema::upid::*;

mod openid;
pub use openid::*;

pub mod remotes;

const_regex! {
    // just a rough check - dummy acceptor is used before persisting
    pub OPENSSL_CIPHERS_REGEX = r"^[0-9A-Za-z_:, +!\-@=.]+$";



    pub SUBSCRIPTION_KEY_REGEX = concat!(r"^pbs(?:[cbsp])-[0-9a-f]{10}$");
}

pub const BLOCKDEVICE_NAME_FORMAT: ApiStringFormat =
    ApiStringFormat::Pattern(&BLOCKDEVICE_NAME_REGEX);
pub const SUBSCRIPTION_KEY_FORMAT: ApiStringFormat =
    ApiStringFormat::Pattern(&SUBSCRIPTION_KEY_REGEX);

pub const HOSTNAME_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&HOSTNAME_REGEX);
pub const OPENSSL_CIPHERS_TLS_FORMAT: ApiStringFormat =
    ApiStringFormat::Pattern(&OPENSSL_CIPHERS_REGEX);
pub const HOST_PORT_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&HOST_PORT_REGEX);
pub const HTTP_URL_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&HTTP_URL_REGEX);


pub const DAILY_DURATION_FORMAT: ApiStringFormat =
    ApiStringFormat::VerifyFn(|s| parse_daily_duration(s).map(drop));


pub const OPENSSL_CIPHERS_TLS_1_2_SCHEMA: Schema =
    StringSchema::new("OpenSSL cipher list used by the api server for TLS <= 1.2")
        .format(&OPENSSL_CIPHERS_TLS_FORMAT)
        .schema();

pub const OPENSSL_CIPHERS_TLS_1_3_SCHEMA: Schema =
    StringSchema::new("OpenSSL ciphersuites list used by the api server for TLS 1.3")
        .format(&OPENSSL_CIPHERS_TLS_FORMAT)
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
serde_plain::derive_fromstr_from_deserialize!(ConfigurationState);

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
