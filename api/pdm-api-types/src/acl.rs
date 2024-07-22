use std::str::FromStr;

use const_format::concatcp;
use serde::de::{value, IntoDeserializer};
use serde::{Deserialize, Serialize};

use proxmox_lang::constnamedbitmap;
use proxmox_schema::api_types::SAFE_ID_REGEX_STR;
use proxmox_schema::{api, const_regex, ApiStringFormat, BooleanSchema, Schema, StringSchema};

const_regex! {
    pub ACL_PATH_REGEX = concatcp!(r"^(?:/|", r"(?:/", SAFE_ID_REGEX_STR, ")+", r")$");
}

// define Privilege bitfield

constnamedbitmap! {
    /// Contains a list of privilege name to privilege value mappings.
    ///
    /// The names are used when displaying/persisting privileges anywhere, the values are used to
    /// allow easy matching of privileges as bitflags.
    PRIVILEGES: u64 => {
        /// `System.Audit` allows knowing about the system and its status.
        PRIV_SYS_AUDIT("System.Audit");
        /// `System.Modify` allows modifying system-level configuration.
        PRIV_SYS_MODIFY("System.Modify");
        /// `Sys.Console` allows access to the system's console
        PRIV_SYS_CONSOLE("Sys.Console");

        /// `Resource.Audit` allows auditing guests, storages and other resources.
        PRIV_RESOURCE_AUDIT("Resource.Audit");
        /// `Resource.Manage` allows managing resources, like starting or stopping guests.
        PRIV_RESOURCE_MANAGE("Resource.Manage");
        /// `Resource.Modify` allows modifying resources, like making configuration changes.
        PRIV_RESOURCE_MODIFY("Resource.Modify");
        /// `Resource.Create` allows creating a guest.
        PRIV_RESOURCE_CREATE("Resource.Create");
        /// `Resource.Delete` allows deleting a guest.
        PRIV_RESOURCE_DELETE("Resource.Delete");
        /// `Resource.Migrate` allows remote migration of a guest.
        PRIV_RESOURCE_MIGRATE("Resource.Migrate");

        /// `Access.Audit` allows auditing permissions and users.
        PRIV_ACCESS_AUDIT("Access.Audit");
        /// `Access.Modify` allows modifying permissions and users.
        PRIV_ACCESS_MODIFY("Access.Modify");
    }
}

pub fn privs_to_priv_names(privs: u64) -> Vec<&'static str> {
    PRIVILEGES
        .iter()
        .fold(Vec::new(), |mut priv_names, (name, value)| {
            if value & privs != 0 {
                priv_names.push(name);
            }
            priv_names
        })
}

#[rustfmt::skip]
#[allow(clippy::identity_op)]
mod roles {
    use super::*;

    /// Admin always has all privileges. It can do everything except a few actions
    /// which are limited to the 'root@pam` superuser
    pub const ROLE_ADMINISTRATOR: u64 = u64::MAX;

    /// NoAccess can be used to remove privileges from specific (sub-)paths
    pub const ROLE_NO_ACCESS: u64 = 0;

    /// Audit can view configuration and status information, but not modify it.
    pub const ROLE_AUDITOR: u64 = 0
        | PRIV_SYS_AUDIT
        | PRIV_RESOURCE_AUDIT
        | PRIV_ACCESS_AUDIT;

    /// The system administrator has `System.Modify` access everywhere.
    pub const ROLE_SYS_ADMINISTRATOR: u64 = 0
        | PRIV_SYS_AUDIT
        | PRIV_SYS_MODIFY;

    /// The system auditor has `System.Audit` access everywhere.
    pub const ROLE_SYS_AUDITOR: u64 = 0
        | PRIV_SYS_AUDIT;
    ///
    /// The resource administrator has `Resource.Modify` access everywhere.
    pub const ROLE_RESOURCE_ADMINISTRATOR: u64 = 0
        | PRIV_RESOURCE_AUDIT
        | PRIV_RESOURCE_MODIFY
        | PRIV_RESOURCE_DELETE
        | PRIV_RESOURCE_MIGRATE;

    /// The resource auditor has `Resource.Audit` access everywhere.
    pub const ROLE_RESOURCE_AUDITOR: u64 = 0
        | PRIV_RESOURCE_AUDIT;

    /// The access auditor has `Access.Audit` access everywhere.
    pub const ROLE_ACCESS_AUDITOR: u64 = 0
        | PRIV_ACCESS_AUDIT;

    /// NoAccess can be used to remove privileges from specific (sub-)paths
    pub const ROLE_NAME_NO_ACCESS: &str = "NoAccess";
}
pub use roles::*;

#[api(type_text: "<role>")]
#[repr(u64)]
#[derive(Serialize, Deserialize)]
/// Enum representing roles via their [PRIVILEGES] combination.
///
/// Since privileges are implemented as bitflags, each unique combination of privileges maps to a
/// single, unique `u64` value that is used in this enum definition.
pub enum Role {
    /// Administrator
    Administrator = ROLE_ADMINISTRATOR,
    /// Auditor
    Auditor = ROLE_AUDITOR,
    /// Disable Access
    NoAccess = ROLE_NO_ACCESS,
}

impl FromStr for Role {
    type Err = value::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::deserialize(s.into_deserializer())
    }
}

pub const ACL_PATH_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&ACL_PATH_REGEX);

pub const ACL_PATH_SCHEMA: Schema = StringSchema::new("Access control path.")
    .format(&ACL_PATH_FORMAT)
    .min_length(1)
    .max_length(128)
    .schema();

pub const ACL_PROPAGATE_SCHEMA: Schema =
    BooleanSchema::new("Allow to propagate (inherit) permissions.")
        .default(true)
        .schema();

#[api]
/// Type of the 'ugid' property in the ACL entry list.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AclUgidType {
    /// An entry for a user (or token).
    User,
    /// An entry for a group.
    Group,
}
serde_plain::derive_display_from_serialize!(AclUgidType);
serde_plain::derive_fromstr_from_deserialize!(AclUgidType);

#[api(
    properties: {
        propagate: { schema: ACL_PROPAGATE_SCHEMA, },
        path: { schema: ACL_PATH_SCHEMA, },
        ugid_type: { type: AclUgidType },
        ugid: {
            type: String,
            description: "User or Group ID.",
        },
        roleid: { type: Role }
    }
)]
#[derive(Serialize, Deserialize)]
/// ACL list entry.
pub struct AclListItem {
    pub path: String,
    pub ugid: String,
    pub ugid_type: AclUgidType,
    pub propagate: bool,
    pub roleid: String,
}
