use anyhow::Error;
use lazy_static::lazy_static;
use openssl::pkey::{PKey, Private, Public};
use openssl::rsa::Rsa;

use proxmox_sys::fs::file_get_contents;

use pdm_buildcfg::configdir;

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
