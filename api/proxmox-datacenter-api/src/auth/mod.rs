use anyhow::{bail, Error};

pub mod csrf;
pub mod key;

pub fn setup_keys() -> Result<(), Error> {
    if let Err(err) = key::generate_auth_key() {
        bail!("unable to generate auth key - {err}");
    }
    if let Err(err) = csrf::generate_csrf_key() {
        bail!("unable to generate csrf key - {err}");
    }
    if let Err(err) = pdm_api_common::auth::certs::update_self_signed_cert(false) {
        bail!("unable to generate TLS certs - {err}");
    }
    Ok(())
}
