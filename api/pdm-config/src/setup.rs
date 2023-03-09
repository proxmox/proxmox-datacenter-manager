//! Setup methods.

use anyhow::{bail, ensure, format_err, Error};
use nix::sys::stat::Mode;
use nix::unistd::{Gid, Uid};

use pdm_buildcfg::configdir;

pub fn create_configdir() -> Result<(), Error> {
    let api_user = crate::api_user()?;

    let cfgdir = pdm_buildcfg::CONFIGDIR;
    mkdir_perms(cfgdir, api_user.uid, api_user.gid, 0o700)?;
    mkdir_perms(configdir!("/auth"), nix::unistd::ROOT, api_user.gid, 0o750)?;

    Ok(())
}

pub fn mkdir_perms(dir: &str, uid: Uid, gid: Gid, mode: u32) -> Result<(), Error> {
    match nix::unistd::mkdir(dir, Mode::from_bits_truncate(mode)) {
        Ok(()) => (),
        Err(nix::errno::Errno::EEXIST) => {
            check_permissions(dir, uid, gid, mode)
                .map_err(|err| format_err!("unexpected permissions directory '{dir}': {err}"))?;
            return Ok(());
        }
        Err(err) => bail!("unable to create directory '{dir}' - {err}",),
    }

    nix::unistd::chown(dir, Some(uid), Some(gid))
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
    ensure!(
        (st_mode & 0o777) == mode,
        "bad permissions (0o{st_mode:o} & 0o700 != 0o{mode:o})"
    );

    Ok(())
}
