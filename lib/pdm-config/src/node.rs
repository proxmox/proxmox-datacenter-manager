use anyhow::Error;
use openssl::ssl::{SslAcceptor, SslMethod};

use proxmox_schema::ApiType;

use proxmox_http::ProxyConfig;

use pdm_api_types::ConfigDigest;

use pdm_buildcfg::configdir;
use proxmox_product_config::{open_api_lockfile, replace_config, ApiLockGuard};

use pdm_api_types::NodeConfig;

const CONF_FILE: &str = configdir!("/node.cfg");
const LOCK_FILE: &str = configdir!("/.node.lck");

pub fn lock() -> Result<ApiLockGuard, Error> {
    open_api_lockfile(LOCK_FILE, None, true)
}

/// Read the Node Config.
pub fn config() -> Result<(NodeConfig, ConfigDigest), Error> {
    let content = proxmox_sys::fs::file_read_optional_string(CONF_FILE)?.unwrap_or_default();

    let digest = openssl::sha::sha256(content.as_bytes());
    let data: NodeConfig = proxmox_simple_config::from_str(&content, &NodeConfig::API_SCHEMA)?;

    Ok((data, digest.into()))
}

/// Write the Node Config, requires the write lock to be held.
pub fn save_config(config: &NodeConfig) -> Result<(), Error> {
    validate_node_config(config)?;

    let raw = proxmox_simple_config::to_bytes(config, &NodeConfig::API_SCHEMA)?;
    replace_config(CONF_FILE, &raw)
}

/// Returns the parsed ProxyConfig
pub fn get_http_proxy_config(config: &NodeConfig) -> Option<ProxyConfig> {
    if let Some(http_proxy) = &config.http_proxy {
        match ProxyConfig::parse_proxy_url(http_proxy) {
            Ok(proxy) => Some(proxy),
            Err(_) => None,
        }
    } else {
        None
    }
}

// Validate the configuration.
fn validate_node_config(config: &NodeConfig) -> Result<(), Error> {
    let mut dummy_acceptor = SslAcceptor::mozilla_intermediate_v5(SslMethod::tls()).unwrap();
    if let Some(ciphers) = config.ciphers_tls_1_3.as_deref() {
        dummy_acceptor.set_ciphersuites(ciphers)?;
    }
    if let Some(ciphers) = config.ciphers_tls_1_2.as_deref() {
        dummy_acceptor.set_cipher_list(ciphers)?;
    }

    Ok(())
}
