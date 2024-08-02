use std::collections::HashMap;
use std::sync::OnceLock;

use anyhow::{Context as _, Error};

use proxmox_access_control::types::User;
use proxmox_auth_api::types::Authid;
use proxmox_section_config::SectionConfigData;

struct AccessControlConfig;

static PRIVILEGES: OnceLock<HashMap<&str, u64>> = OnceLock::new();
static ROLES: OnceLock<HashMap<&str, u64>> = OnceLock::new();

impl proxmox_access_control::init::AccessControlConfig for AccessControlConfig {
    fn privileges(&self) -> &HashMap<&str, u64> {
        PRIVILEGES.get_or_init(|| pdm_api_types::PRIVILEGES.into_iter().copied().collect())
    }

    #[rustfmt::skip]
    fn roles(&self) -> &HashMap<&str, u64> {
        ROLES.get_or_init(|| {
            [
                ("Administrator", pdm_api_types::ROLE_ADMINISTRATOR),
                ("Auditor", pdm_api_types::ROLE_AUDITOR),
                ("SystemAdministrator", pdm_api_types::ROLE_SYS_ADMINISTRATOR),
                ("SystemAuditor", pdm_api_types::ROLE_SYS_AUDITOR),
                ("ResourceAdministrator", pdm_api_types::ROLE_RESOURCE_ADMINISTRATOR),
                ("ResourceAuditor", pdm_api_types::ROLE_RESOURCE_AUDITOR),
                ("AccessAuditor", pdm_api_types::ROLE_ACCESS_AUDITOR),
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
}

pub(crate) fn init() {
    static ACCESS_CONTROL_CONFIG: AccessControlConfig = AccessControlConfig;

    proxmox_access_control::init::init(&ACCESS_CONTROL_CONFIG, pdm_buildcfg::configdir!("/access"))
        .expect("failed to setup access control config");
}
