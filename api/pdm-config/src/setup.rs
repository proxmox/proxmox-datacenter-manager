//! Setup methods.

//use std::os::fd::OwnedFd;
use std::os::unix::io::{AsRawFd, FromRawFd, OwnedFd};

use anyhow::{bail, ensure, format_err, Error};
use nix::fcntl::OFlag;
use nix::sys::stat::Mode;
use nix::unistd::{Gid, Uid};

use pdm_buildcfg::configdir;

pub fn create_configdir() -> Result<(), Error> {
    let api_user = crate::api_user()?;

    let cfgdir = pdm_buildcfg::CONFIGDIR;
    mkdir_perms(cfgdir, api_user.uid, api_user.gid, 0o770)?;
    mkdir_perms(configdir!("/auth"), nix::unistd::ROOT, api_user.gid, 0o750)?;

    Ok(())
}

pub fn mkdir_perms(dir: &str, uid: Uid, gid: Gid, mode: u32) -> Result<(), Error> {
    let nix_mode = Mode::from_bits(mode).expect("bad mode bits for nix crate");
    match nix::unistd::mkdir(dir, nix_mode) {
        Ok(()) => (),
        Err(nix::errno::Errno::EEXIST) => {
            check_permissions(dir, uid, gid, mode)
                .map_err(|err| format_err!("unexpected permissions directory '{dir}': {err}"))?;
            return Ok(());
        }
        Err(err) => bail!("unable to create directory '{dir}' - {err}",),
    }

    let fd = nix::fcntl::open(dir, OFlag::O_DIRECTORY, Mode::empty())
        .map(|fd| unsafe { OwnedFd::from_raw_fd(fd) })
        .map_err(|err| format_err!("unable to open created directory '{dir}' - {err}"))?;
    // umask defaults to 022 so make sure the mode is fully honowed:
    nix::sys::stat::fchmod(fd.as_raw_fd(), nix_mode)
        .map_err(|err| format_err!("unable to set mode for directory '{dir}' - {err}"))?;
    nix::unistd::fchown(fd.as_raw_fd(), Some(uid), Some(gid))
        .map_err(|err| format_err!("unable to set ownership directory '{dir}' - {err}"))?;

    Ok(())
}

pub fn check_permissions(dir: &str, uid: Uid, gid: Gid, mode: u32) -> Result<(), Error> {
    let uid = uid.as_raw();
    let gid = gid.as_raw();

    let nix::sys::stat::FileStat {
        st_uid,
        st_gid,
        st_mode,
        ..
    } = nix::sys::stat::stat(dir)?;

    ensure!(st_uid == uid, "bad owner ({st_uid} != {uid})");
    ensure!(st_gid == gid, "bad group ({st_gid} != {gid})");
    let perms = st_mode & !nix::sys::stat::SFlag::S_IFMT.bits();
    ensure!(perms == mode, "bad permissions (0o{perms:o} != 0o{mode:o})");

    Ok(())
}
