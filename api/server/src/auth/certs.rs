use anyhow::{format_err, Error};
use std::path::PathBuf;

use proxmox_dns_api::read_etc_resolv_conf;

use pdm_buildcfg::configdir;

pub const API_KEY_FN: &str = configdir!("/auth/api.key");
pub const API_CERT_FN: &str = configdir!("/auth/api.pem");

/// Update self signed node certificate.
pub fn update_self_signed_cert(force: bool) -> Result<(), Error> {
    let key_path = PathBuf::from(API_KEY_FN);
    let cert_path = PathBuf::from(API_CERT_FN);

    if key_path.exists() && cert_path.exists() && !force {
        return Ok(());
    }

    let resolv_conf = read_etc_resolv_conf(None)?.config;

    let (priv_key, cert) = proxmox_acme_api::create_self_signed_cert(
        "Proxmox Backup Server",
        proxmox_sys::nodename(),
        resolv_conf.search.as_deref(),
    )?;

    let cert_pem = cert.to_pem()?;
    let priv_pem = priv_key.private_key_to_pem_pkcs8()?;

    set_api_certificate(&cert_pem, &priv_pem)?;

    Ok(())
}

pub(crate) fn set_api_certificate(cert_pem: &[u8], key_pem: &[u8]) -> Result<(), Error> {
    let key_path = PathBuf::from(API_KEY_FN);
    let cert_path = PathBuf::from(API_CERT_FN);

    proxmox_product_config::replace_privileged_config(key_path, key_pem)
        .map_err(|err| format_err!("error writing certificate private key - {}", err))?;
    proxmox_product_config::replace_privileged_config(cert_path, cert_pem)
        .map_err(|err| format_err!("error writing certificate file - {}", err))?;

    Ok(())
}
