pub(crate) fn init() {
    static ACCESS_CONTROL_CONFIG: pdm_api_types::AccessControlConfig =
        pdm_api_types::AccessControlConfig;

    proxmox_access_control::init::init(&ACCESS_CONTROL_CONFIG, pdm_buildcfg::configdir!("/access"))
        .expect("failed to setup access control config");
}
