use pdm_buildcfg::{PROXMOX_PKG_RELEASE, PROXMOX_PKG_VERSION};
use proxmox_upgrade_checks::UpgradeCheckerBuilder;

fn main() -> Result<(), anyhow::Error> {
    UpgradeCheckerBuilder::new(
        "bookworm",
        "trixie",
        "proxmox-datacenter-manager",
        0,
        1,
        11,
        &format!("{PROXMOX_PKG_VERSION}.{PROXMOX_PKG_RELEASE}"),
    )
    .apt_state_file_location(pdm_buildcfg::APT_PKG_STATE_FN)
    .add_service_to_checks("proxmox-datacenter-api")
    .add_service_to_checks("proxmox-datacenter-privileged-api")
    .build()
    .run()
}
