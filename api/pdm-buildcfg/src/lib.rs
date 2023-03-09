//! Exports configuration data from the build system

pub const PROXMOX_PKG_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION_MAJOR"),
    ".",
    env!("CARGO_PKG_VERSION_MINOR"),
);
pub const PROXMOX_PKG_RELEASE: &str = env!("CARGO_PKG_VERSION_PATCH");
pub const PROXMOX_PKG_REPOID: &str = env!("REPOID");

/// The configured configuration directory
pub const CONFIGDIR: &str = "/etc/proxmox-datacenter-manager";
pub const JS_DIR: &str = "/usr/share/javascript/proxmox-datacenter-manager";

// FIXME use (and add) pdm user?
/// Unix system user used by proxmox-backup-proxy
pub const BACKUP_USER_NAME: &str = "www-data";
/// Unix system group used by proxmox-backup-proxy
pub const BACKUP_GROUP_NAME: &str = "www-data";

#[macro_export]
macro_rules! PDM_RUN_DIR_M {
    () => {
        "/run/proxmox-datacenter-manager"
    };
}

#[macro_export]
macro_rules! PDM_STATE_DIR_M {
    () => {
        "/var/lib/proxmox-datacenter-manager"
    };
}

#[macro_export]
macro_rules! PDM_LOG_DIR_M {
    () => {
        "/var/log/proxmox-datacenter-manager"
    };
}

#[macro_export]
macro_rules! PDM_CACHE_DIR_M {
    () => {
        "/var/cache/proxmox-datacenter-manager"
    };
}

/// namespaced directory for in-memory (tmpfs) run state
pub const PDM_RUN_DIR: &str = PDM_RUN_DIR_M!();

/// namespaced directory for persistent cache
pub const PDM_CACHE_DIR: &str = PDM_CACHE_DIR_M!();

/// namespaced directory for persistent state
pub const PDM_STATE_DIR: &str = PDM_STATE_DIR_M!();

/// namespaced directory for persistent logging
pub const PDM_LOG_DIR: &str = PDM_LOG_DIR_M!();

/// logfile for all API requests handled by the proxy and privileged API daemons. Note that not all
/// failed logins can be logged here with full information, use the auth log for that.
pub const API_ACCESS_LOG_FN: &str = concat!(PDM_LOG_DIR_M!(), "/api/access.log");

/// logfile for any failed authentication, via ticket or via token, and new successful ticket
/// creations. This file can be useful for fail2ban.
pub const API_AUTH_LOG_FN: &str = concat!(PDM_LOG_DIR_M!(), "/api/auth.log");

/// the PID filename for the unprivileged proxy daemon
pub const PDM_PROXY_PID_FN: &str = concat!(PDM_RUN_DIR_M!(), "/proxy.pid");

/// the PID filename for the privileged api daemon
pub const PDM_PRIV_PID_FN: &str = concat!(PDM_RUN_DIR_M!(), "/priv.pid");

/// The privileged api socket file.
pub const PDM_PRIV_SOCKET_FN: &str = concat!(PDM_RUN_DIR_M!(), "/api.sock");

pub const PDM_SUBSCRIPTION_FN: &str = configdir!("/subscription");

/// Prepend configuration directory to a file name
///
/// This is a simply way to get the full path for configuration files.
/// #### Example:
/// ```
/// use pbs_buildcfg::configdir;
/// let cert_path = configdir!("/proxy.pfx");
/// ```
#[macro_export]
macro_rules! configdir {
    ($subdir:expr) => {
        concat!("/etc/proxmox-datacenter-manager", $subdir)
    };
}

/// Prepend the run directory to a file name.
///
/// This is a simply way to get the full path for files in `/run`.
#[macro_export]
macro_rules! rundir {
    ($subdir:expr) => {
        concat!($crate::PDM_RUN_DIR_M!(), $subdir)
    };
}
