use std::collections::HashMap;
use std::str::FromStr;
use std::sync::LazyLock;

use anyhow::{format_err, Context, Error};
use const_format::concatcp;
use serde::de::{value, IntoDeserializer};
use serde::{Deserialize, Serialize};

use proxmox_access_control::types::User;
use proxmox_auth_api::types::Authid;
use proxmox_lang::constnamedbitmap;
use proxmox_schema::api_types::SAFE_ID_REGEX_STR;
use proxmox_schema::{api, const_regex, ApiStringFormat, BooleanSchema, Schema, StringSchema};
use proxmox_section_config::SectionConfigData;

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
        /// `Sys.PowerManagement` allows powering off or rebooting the system.
        PRIV_SYS_POWER_MANAGEMENT("Sys.PowerManagement");

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

        /// Realm.Allocate allows viewing, creating, modifying and deleting realms
        PRIV_REALM_ALLOCATE("Realm.Allocate");
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

    ///// The system administrator has `System.Modify` access everywhere.
    //pub const ROLE_SYS_ADMINISTRATOR: u64 = 0
    //    | PRIV_SYS_AUDIT
    //    | PRIV_SYS_MODIFY;

    ///// The system auditor has `System.Audit` access everywhere.
    //pub const ROLE_SYS_AUDITOR: u64 = 0
    //    | PRIV_SYS_AUDIT;
    /////
    ///// The resource administrator has `Resource.Modify` access everywhere.
    //pub const ROLE_RESOURCE_ADMINISTRATOR: u64 = 0
    //    | PRIV_RESOURCE_AUDIT
    //    | PRIV_RESOURCE_MODIFY
    //    | PRIV_RESOURCE_DELETE
    //    | PRIV_RESOURCE_MIGRATE;

    ///// The resource auditor has `Resource.Audit` access everywhere.
    //pub const ROLE_RESOURCE_AUDITOR: u64 = 0
    //    | PRIV_RESOURCE_AUDIT;

    ///// The access auditor has `Access.Audit` access everywhere.
    //pub const ROLE_ACCESS_AUDITOR: u64 = 0
    //    | PRIV_ACCESS_AUDIT;

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

pub struct AccessControlConfig;

impl proxmox_access_control::init::AccessControlConfig for AccessControlConfig {
    fn privileges(&self) -> &HashMap<&str, u64> {
        static PRIVS: LazyLock<HashMap<&str, u64>> =
            LazyLock::new(|| PRIVILEGES.iter().copied().collect());

        &PRIVS
    }

    #[rustfmt::skip]
    fn roles(&self) -> &HashMap<&str, (u64, &str)> {
        static ROLES: LazyLock<HashMap<&str, (u64, &str)>> = LazyLock::new(|| {
            [
                ("Administrator", (ROLE_ADMINISTRATOR, "Administrators can inspect and modify the system.")),
                ("Auditor", (ROLE_AUDITOR, "An Auditor can inspect many aspects of the system, but not change them.")),
                //("SystemAdministrator", pdm_api_types::ROLE_SYS_ADMINISTRATOR),
                //("SystemAuditor", pdm_api_types::ROLE_SYS_AUDITOR),
                //("ResourceAdministrator", pdm_api_types::ROLE_RESOURCE_ADMINISTRATOR),
                //("ResourceAuditor", pdm_api_types::ROLE_RESOURCE_AUDITOR),
                //("AccessAuditor", pdm_api_types::ROLE_ACCESS_AUDITOR),
            ]
            .into_iter()
            .collect()
        });

        &ROLES
    }

    fn is_superuser(&self, auth_id: &Authid) -> bool {
        !auth_id.is_token() && auth_id.user() == "root@pam"
    }

    fn role_admin(&self) -> Option<&str> {
        Some("Administrator")
    }

    fn init_user_config(&self, config: &mut SectionConfigData) -> Result<(), Error> {
        if !config.sections.contains_key("root@pam") {
            config
                .set_data(
                    "root@pam",
                    "user",
                    User {
                        userid: "root@pam".parse().expect("invalid user id"),
                        comment: Some("Superuser".to_string()),
                        enable: None,
                        expire: None,
                        firstname: None,
                        lastname: None,
                        email: None,
                    },
                )
                .context("failed to insert default user into user config")?
        }

        Ok(())
    }

    fn acl_audit_privileges(&self) -> u64 {
        PRIV_ACCESS_AUDIT
    }

    fn acl_modify_privileges(&self) -> u64 {
        PRIV_ACCESS_MODIFY
    }
    fn check_acl_path(&self, path: &str) -> Result<(), Error> {
        let components = proxmox_access_control::acl::split_acl_path(path);

        let components_len = components.len();

        if components_len == 0 {
            return Ok(());
        }
        match components[0] {
            "access" => {
                if components_len == 1 {
                    return Ok(());
                }
                match components[1] {
                    "acl" | "users" | "realm" => {
                        if components_len == 2 {
                            return Ok(());
                        }
                    }
                    _ => {}
                }
            }
            "resource" => {
                // `/resource` and `/resource/{remote}`
                if components_len <= 2 {
                    return Ok(());
                }
                // `/resource/{remote-id}/{resource-type=guest,storage}/{resource-id}`
                match components[2] {
                    "guest" | "storage" => {
                        // /resource/{remote-id}/{resource-type}
                        // /resource/{remote-id}/{resource-type}/{resource-id}
                        if components_len <= 4 {
                            return Ok(());
                        }
                    }
                    _ => {}
                }
            }
            "system" => {
                if components_len == 1 {
                    return Ok(());
                }
                match components[1] {
                    "certificates" | "disks" | "log" | "notifications" | "status" | "tasks"
                    | "time" => {
                        if components_len == 2 {
                            return Ok(());
                        }
                    }
                    "services" => {
                        // /system/services/{service}
                        if components_len <= 3 {
                            return Ok(());
                        }
                    }
                    "network" => {
                        if components_len == 2 {
                            return Ok(());
                        }
                        match components[2] {
                            "dns" => {
                                if components_len == 3 {
                                    return Ok(());
                                }
                            }
                            "interfaces" => {
                                // /system/network/interfaces/{iface}
                                if components_len <= 4 {
                                    return Ok(());
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            "view" => {
                // `/view` and `/view/{view-id}`
                if components_len <= 2 {
                    return Ok(());
                }
            }
            _ => {}
        }

        Err(format_err!("invalid acl path '{}'.", path))
    }
}
