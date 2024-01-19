use anyhow::{bail, format_err, Error};
use nix::unistd::{Gid, Group, Uid, User};

pub use pdm_buildcfg::{BACKUP_GROUP_NAME, BACKUP_USER_NAME};

pub mod section_config;

pub mod acl;
pub mod domains;
pub mod remotes;
pub mod setup;
pub mod token_shadow;
pub mod user;

mod config_version_cache;
pub use config_version_cache::ConfigVersionCache;

mod cached_user_info;
pub use cached_user_info::CachedUserInfo;

/// A configuration digest - a SHA256 hash.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigDigest([u8; 32]);

impl ConfigDigest {
    pub fn to_hex(&self) -> String {
        hex::encode(&self.0)
    }
}

impl From<[u8; 32]> for ConfigDigest {
    #[inline]
    fn from(digest: [u8; 32]) -> Self {
        Self(digest)
    }
}

impl Into<[u8; 32]> for ConfigDigest {
    #[inline]
    fn into(self) -> [u8; 32] {
        self.0
    }
}

impl AsRef<[u8]> for ConfigDigest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8; 32]> for ConfigDigest {
    fn as_ref(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::ops::Deref for ConfigDigest {
    type Target = [u8; 32];

    fn deref(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::ops::DerefMut for ConfigDigest {
    fn deref_mut(&mut self) -> &mut [u8; 32] {
        &mut self.0
    }
}

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

#[allow(dead_code)]
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

/// Atomically write data to file owned by `root:api-user` with permission `0640`
///
/// Only the superuser can write those files, but group 'api-user' can read them.
pub fn replace_privileged_config<P: AsRef<std::path::Path>>(
    path: P,
    data: &[u8],
) -> Result<(), Error> {
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

/// Atomically write data to file owned by `api-user:api-user` with permission `0660`.
pub fn replace_config<P: AsRef<std::path::Path>>(path: P, data: &[u8]) -> Result<(), Error> {
    let api_user = api_user()?;
    let mode = nix::sys::stat::Mode::from_bits_truncate(0o0640);
    // set the correct owner/group/permissions while saving file
    // owner(rw) = root, group(r)= api-user
    let options = proxmox_sys::fs::CreateOptions::new()
        .perm(mode)
        .owner(api_user.uid)
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

/// Detect modified configuration files
///
/// This function fails with a reasonable error message if checksums do not match.
pub fn detect_modified_configuration_file(
    user_digest: Option<&str>,
    config_digest: &[u8; 32],
) -> Result<(), Error> {
    use hex::FromHex;

    let user_digest = match user_digest {
        Some(digest) => <[u8; 32]>::from_hex(digest)?,
        None => return Ok(()),
    };

    if user_digest != *config_digest {
        bail!("detected modified configuration - file changed by other user? Try again.");
    }
    Ok(())
}
