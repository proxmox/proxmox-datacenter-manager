pub mod acl;
mod cached_user_info;
pub use cached_user_info::CachedUserInfo;
pub mod domains;
pub mod token_shadow;
pub mod user;

mod config_version_cache;
pub use config_version_cache::ConfigVersionCache;

use anyhow::{format_err, Error};
use nix::unistd::{Gid, Group, Uid, User};

pub use pdm_buildcfg::{BACKUP_GROUP_NAME, BACKUP_USER_NAME};

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

pub struct ApiLockGuard(Option<std::fs::File>);

#[doc(hidden)]
/// Note: do not use for production code, this is only intended for tests
pub unsafe fn create_mocked_lock() -> ApiLockGuard {
    ApiLockGuard(None)
}

/// Open or create a lock file owned by user "api-user" and lock it.
///
/// Owner/Group of the file is set to api-user/api-group.
/// File mode is 0660.
/// Default timeout is 10 seconds.
///
/// Note: This method needs to be called by user "root" or "api-user".
pub fn open_api_lockfile<P: AsRef<std::path::Path>>(
    path: P,
    timeout: Option<std::time::Duration>,
    exclusive: bool,
) -> Result<ApiLockGuard, Error> {
    let user = api_user()?;
    let options = proxmox_sys::fs::CreateOptions::new()
        .perm(nix::sys::stat::Mode::from_bits_truncate(0o660))
        .owner(user.uid)
        .group(user.gid);

    let timeout = timeout.unwrap_or(std::time::Duration::new(10, 0));

    let file = proxmox_sys::fs::open_file_locked(&path, timeout, exclusive, options)?;
    Ok(ApiLockGuard(Some(file)))
}

/// Atomically write data to file owned by "root:api-user" with permission "0640"
///
/// Only the superuser can write those files, but group 'api-user' can read them.
pub fn replace_api_config<P: AsRef<std::path::Path>>(path: P, data: &[u8]) -> Result<(), Error> {
    let api_user = api_user()?;
    let mode = nix::sys::stat::Mode::from_bits_truncate(0o0640);
    // set the correct owner/group/permissions while saving file
    // owner(rw) = root, group(r)= api-user
    let options = proxmox_sys::fs::CreateOptions::new()
        .perm(mode)
        .owner(nix::unistd::ROOT)
        .group(api_user.gid);

    proxmox_sys::fs::replace_file(path, data, options, true)?;

    Ok(())
}

/// Atomically write data to file owned by "root:root" with permission "0600"
///
/// Only the superuser can read and write those files.
pub fn replace_secret_config<P: AsRef<std::path::Path>>(path: P, data: &[u8]) -> Result<(), Error> {
    let mode = nix::sys::stat::Mode::from_bits_truncate(0o0600);
    // set the correct owner/group/permissions while saving file
    // owner(rw) = root, group(r)= root
    let options = proxmox_sys::fs::CreateOptions::new()
        .perm(mode)
        .owner(nix::unistd::ROOT)
        .group(nix::unistd::Gid::from_raw(0));

    proxmox_sys::fs::replace_file(path, data, options, true)?;

    Ok(())
}
