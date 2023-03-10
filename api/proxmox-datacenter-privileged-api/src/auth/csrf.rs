use std::path::PathBuf;

use anyhow::Error;
use openssl::rsa::Rsa;

use proxmox_sys::fs::{replace_file, CreateOptions};

use pdm_buildcfg::configdir;

pub fn generate_csrf_key() -> Result<(), Error> {
    let path = PathBuf::from(configdir!("/auth/csrf.key"));

    if path.exists() {
        return Ok(());
    }

    let rsa = Rsa::generate(2048).unwrap();

    let pem = rsa.private_key_to_pem()?;

    use nix::sys::stat::Mode;

    let api_user = pdm_config::api_user()?;

    replace_file(
        &path,
        &pem,
        CreateOptions::new()
            .perm(Mode::from_bits_truncate(0o0640))
            .owner(nix::unistd::ROOT)
            .group(api_user.gid),
        true,
    )?;

    Ok(())
}
