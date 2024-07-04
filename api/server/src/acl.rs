use std::collections::HashMap;
use std::sync::OnceLock;

use proxmox_auth_api::types::Authid;

struct AccessControlConfig;

static PRIVILEGES: OnceLock<HashMap<&str, u64>> = OnceLock::new();
static ROLES: OnceLock<HashMap<&str, u64>> = OnceLock::new();

impl proxmox_access_control::init::AccessControlConfig for AccessControlConfig {
    fn privileges(&self) -> &HashMap<&str, u64> {
        PRIVILEGES.get_or_init(|| pdm_api_types::PRIVILEGES.into_iter().copied().collect())
    }

    fn roles(&self) -> &HashMap<&str, u64> {
        ROLES.get_or_init(|| {
            [
                ("Admin", pdm_api_types::ROLE_ADMIN),
                ("Audit", pdm_api_types::ROLE_AUDIT),
            ]
            .into_iter()
            .collect()
        })
    }

    fn is_superuser(&self, auth_id: &Authid) -> bool {
        !auth_id.is_token() && auth_id.user() == "root@pam"
    }

    fn role_admin(&self) -> Option<&str> {
        Some("Admin")
    }
}

pub(crate) fn init() {
    static ACCESS_CONTROL_CONFIG: AccessControlConfig = AccessControlConfig;

    proxmox_access_control::init::init(&ACCESS_CONTROL_CONFIG, pdm_buildcfg::configdir!("/access"))
        .expect("failed to setup access control config");
}
