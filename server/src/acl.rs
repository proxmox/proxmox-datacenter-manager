use std::collections::HashMap;
use std::sync::OnceLock;

use anyhow::{format_err, Context as _, Error};

use proxmox_access_control::types::User;
use proxmox_auth_api::types::Authid;
use proxmox_section_config::SectionConfigData;

struct AccessControlConfig;

static PRIVILEGES: OnceLock<HashMap<&str, u64>> = OnceLock::new();
static ROLES: OnceLock<HashMap<&str, (u64, &str)>> = OnceLock::new();

impl proxmox_access_control::init::AccessControlConfig for AccessControlConfig {
    fn privileges(&self) -> &HashMap<&str, u64> {
        PRIVILEGES.get_or_init(|| pdm_api_types::PRIVILEGES.iter().copied().collect())
    }

    #[rustfmt::skip]
    fn roles(&self) -> &HashMap<&str, (u64, &str)> {
        ROLES.get_or_init(|| {
            [
                ("Administrator", (pdm_api_types::ROLE_ADMINISTRATOR, "Administrators can inspect and modify the system.")),
                ("Auditor", (pdm_api_types::ROLE_AUDITOR, "An Auditor can inspect many aspects of the system, but not change them.")),
                //("SystemAdministrator", pdm_api_types::ROLE_SYS_ADMINISTRATOR),
                //("SystemAuditor", pdm_api_types::ROLE_SYS_AUDITOR),
                //("ResourceAdministrator", pdm_api_types::ROLE_RESOURCE_ADMINISTRATOR),
                //("ResourceAuditor", pdm_api_types::ROLE_RESOURCE_AUDITOR),
                //("AccessAuditor", pdm_api_types::ROLE_ACCESS_AUDITOR),
            ]
            .into_iter()
            .collect()
        })
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
        pdm_api_types::PRIV_ACCESS_AUDIT
    }

    fn acl_modify_privileges(&self) -> u64 {
        pdm_api_types::PRIV_ACCESS_MODIFY
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
            _ => {}
        }

        Err(format_err!("invalid acl path '{}'.", path))
    }
}

pub(crate) fn init() {
    static ACCESS_CONTROL_CONFIG: AccessControlConfig = AccessControlConfig;

    proxmox_access_control::init::init(&ACCESS_CONTROL_CONFIG, pdm_buildcfg::configdir!("/access"))
        .expect("failed to setup access control config");
}
