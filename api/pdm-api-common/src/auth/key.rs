use std::path::PathBuf;

use anyhow::Error;
use lazy_static::lazy_static;
use openssl::pkey::{PKey, Private, Public};
use openssl::rsa::Rsa;

use proxmox_sys::fs::{file_get_contents, replace_file, CreateOptions};

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

fn load_public_auth_key() -> Result<PKey<Public>, Error> {
    let pem = file_get_contents(configdir!("/auth/authkey.pub"))?;
    let rsa = Rsa::public_key_from_pem(&pem)?;
    let key = PKey::from_rsa(rsa)?;

    Ok(key)
}

pub fn public_auth_key() -> &'static PKey<Public> {
    lazy_static! {
        static ref KEY: PKey<Public> = load_public_auth_key().unwrap();
    }

    &KEY
}

fn load_private_auth_key() -> Result<PKey<Private>, Error> {
    let pem = file_get_contents(configdir!("/auth/authkey.key"))?;
    let rsa = Rsa::private_key_from_pem(&pem)?;
    let key = PKey::from_rsa(rsa)?;

    Ok(key)
}

pub fn private_auth_key() -> &'static PKey<Private> {
    lazy_static! {
        static ref KEY: PKey<Private> = load_private_auth_key().unwrap();
    }

    &KEY
}
