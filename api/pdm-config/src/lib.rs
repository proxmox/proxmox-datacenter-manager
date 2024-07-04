use anyhow::{format_err, Error};
use nix::unistd::{Gid, Group, Uid, User};

pub use pdm_buildcfg::{BACKUP_GROUP_NAME, BACKUP_USER_NAME};

pub mod certificate_config;
pub mod domains;
pub mod remotes;
pub mod setup;

mod config_version_cache;
pub use config_version_cache::ConfigVersionCache;

/// Return User info for the main system user (``getpwnam_r(3)``)
pub fn api_user() -> Result<nix::unistd::User, Error> {
    if cfg!(test) {
        Ok(User::from_uid(Uid::current())?.expect("current user does not exist"))
    } else {
        User::from_name(BACKUP_USER_NAME)?
            .ok_or_else(|| format_err!("Unable to lookup '{}' user.", BACKUP_USER_NAME))
    }
}

/// Return Group info for the main system group (``getgrnam(3)``)
pub fn api_group() -> Result<nix::unistd::Group, Error> {
    if cfg!(test) {
        Ok(Group::from_gid(Gid::current())?.expect("current group does not exist"))
    } else {
        Group::from_name(BACKUP_GROUP_NAME)?
            .ok_or_else(|| format_err!("Unable to lookup '{}' group.", BACKUP_GROUP_NAME))
    }
}

pub fn priv_user() -> Result<nix::unistd::User, Error> {
    if cfg!(test) {
        Ok(User::from_uid(Uid::current())?.expect("current user does not exist"))
    } else {
        User::from_name("root")?.ok_or_else(|| format_err!("Unable to lookup superuser."))
    }
}
