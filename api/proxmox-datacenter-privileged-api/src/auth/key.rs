use std::path::PathBuf;

use anyhow::Error;
use openssl::rsa::Rsa;

use proxmox_sys::fs::{replace_file, CreateOptions};

use pdm_buildcfg::configdir;

pub fn generate_auth_key() -> Result<(), Error> {
    let priv_path = PathBuf::from(configdir!("/auth/authkey.key"));

    let mut public_path = priv_path.clone();
    public_path.set_extension("pub");

    if priv_path.exists() && public_path.exists() {
        return Ok(());
    }

    let rsa = Rsa::generate(4096).unwrap();

    let priv_pem = rsa.private_key_to_pem()?;

    use nix::sys::stat::Mode;

    replace_file(
        &priv_path,
        &priv_pem,
        CreateOptions::new().perm(Mode::from_bits_truncate(0o0600)),
        true,
    )?;

    let public_pem = rsa.public_key_to_pem()?;

    let api_user = pdm_config::api_user()?;

    replace_file(
        &public_path,
        &public_pem,
        CreateOptions::new()
            .perm(Mode::from_bits_truncate(0o0640))
            .owner(nix::unistd::ROOT)
            .group(api_user.gid),
        true,
    )?;

    Ok(())
}
