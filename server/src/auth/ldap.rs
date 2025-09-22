use std::future::Future;
use std::net::IpAddr;
use std::path::PathBuf;
use std::pin::Pin;

use anyhow::Error;
use pdm_buildcfg::configdir;
use proxmox_auth_api::api::Authenticator;
use proxmox_ldap::types::{AdRealmConfig, LdapMode, LdapRealmConfig};
use proxmox_ldap::{Config, Connection, ConnectionMode};
use proxmox_product_config::ApiLockGuard;
use proxmox_router::http_bail;
use serde_json::json;

use pdm_api_types::UsernameRef;

const LDAP_PASSWORDS_FILENAME: &str = configdir!("/ldap_passwords.json");

#[allow(clippy::upper_case_acronyms)]
pub(crate) struct LdapAuthenticator {
    config: LdapRealmConfig,
}

impl LdapAuthenticator {
    pub(crate) fn new(config: LdapRealmConfig) -> Self {
        Self { config }
    }
}

impl Authenticator for LdapAuthenticator {
    /// Authenticate user in LDAP realm
    fn authenticate_user<'a>(
        &'a self,
        username: &'a UsernameRef,
        password: &'a str,
        _client_ip: Option<&'a IpAddr>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>> {
        Box::pin(async move {
            let ldap_config = Self::api_type_to_config(&self.config)?;
            let ldap = Connection::new(ldap_config);
            ldap.authenticate_user(username.as_str(), password).await?;
            Ok(())
        })
    }

    fn store_password(
        &self,
        _username: &UsernameRef,
        _password: &str,
        _client_ip: Option<&IpAddr>,
    ) -> Result<(), Error> {
        http_bail!(
            NOT_IMPLEMENTED,
            "storing passwords is not implemented for LDAP realms"
        );
    }

    fn remove_password(&self, _username: &UsernameRef) -> Result<(), Error> {
        http_bail!(
            NOT_IMPLEMENTED,
            "removing passwords is not implemented for LDAP realms"
        );
    }
}

impl LdapAuthenticator {
    pub fn api_type_to_config(config: &LdapRealmConfig) -> Result<Config, Error> {
        Self::api_type_to_config_with_password(config, get_ldap_bind_password(&config.realm)?)
    }

    pub fn api_type_to_config_with_password(
        config: &LdapRealmConfig,
        password: Option<String>,
    ) -> Result<Config, Error> {
        let mut servers = vec![config.server1.clone()];
        if let Some(server) = &config.server2 {
            servers.push(server.clone());
        }

        let (ca_store, trusted_cert) = lookup_ca_store_or_cert_path(config.capath.as_deref());

        Ok(Config {
            servers,
            port: config.port,
            user_attr: config.user_attr.clone(),
            base_dn: config.base_dn.clone(),
            bind_dn: config.bind_dn.clone(),
            bind_password: password,
            tls_mode: ldap_to_conn_mode(config.mode.unwrap_or_default()),
            verify_certificate: config.verify.unwrap_or_default(),
            additional_trusted_certificates: trusted_cert,
            certificate_store_path: ca_store,
        })
    }
}

pub struct AdAuthenticator {
    config: AdRealmConfig,
}

impl AdAuthenticator {
    pub(crate) fn new(config: AdRealmConfig) -> Self {
        Self { config }
    }

    pub fn api_type_to_config(config: &AdRealmConfig) -> Result<Config, Error> {
        Self::api_type_to_config_with_password(config, get_ldap_bind_password(&config.realm)?)
    }

    pub fn api_type_to_config_with_password(
        config: &AdRealmConfig,
        password: Option<String>,
    ) -> Result<Config, Error> {
        let mut servers = vec![config.server1.clone()];
        if let Some(server) = &config.server2 {
            servers.push(server.clone());
        }

        let (ca_store, trusted_cert) = lookup_ca_store_or_cert_path(config.capath.as_deref());

        Ok(Config {
            servers,
            port: config.port,
            user_attr: "sAMAccountName".to_owned(),
            base_dn: config.base_dn.clone().unwrap_or_default(),
            bind_dn: config.bind_dn.clone(),
            bind_password: password,
            tls_mode: ldap_to_conn_mode(config.mode.unwrap_or_default()),
            verify_certificate: config.verify.unwrap_or_default(),
            additional_trusted_certificates: trusted_cert,
            certificate_store_path: ca_store,
        })
    }
}

impl Authenticator for AdAuthenticator {
    /// Authenticate user in AD realm
    fn authenticate_user<'a>(
        &'a self,
        username: &'a UsernameRef,
        password: &'a str,
        _client_ip: Option<&'a IpAddr>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>> {
        Box::pin(async move {
            let ldap_config = Self::api_type_to_config(&self.config)?;
            let ldap = Connection::new(ldap_config);
            ldap.authenticate_user(username.as_str(), password).await?;
            Ok(())
        })
    }

    fn store_password(
        &self,
        _username: &UsernameRef,
        _password: &str,
        _client_ip: Option<&IpAddr>,
    ) -> Result<(), Error> {
        http_bail!(
            NOT_IMPLEMENTED,
            "storing passwords is not implemented for Active Directory realms"
        );
    }

    fn remove_password(&self, _username: &UsernameRef) -> Result<(), Error> {
        http_bail!(
            NOT_IMPLEMENTED,
            "removing passwords is not implemented for Active Directory realms"
        );
    }
}

fn ldap_to_conn_mode(mode: LdapMode) -> ConnectionMode {
    match mode {
        LdapMode::Ldap => ConnectionMode::Ldap,
        LdapMode::StartTls => ConnectionMode::StartTls,
        LdapMode::Ldaps => ConnectionMode::Ldaps,
    }
}

fn lookup_ca_store_or_cert_path(capath: Option<&str>) -> (Option<PathBuf>, Option<Vec<PathBuf>>) {
    if let Some(capath) = capath {
        let path = PathBuf::from(capath);
        if path.is_dir() {
            (Some(path), None)
        } else {
            (None, Some(vec![path]))
        }
    } else {
        (None, None)
    }
}

/// Store LDAP bind passwords in protected file. The domain config must be locked while this
/// function is executed.
pub(crate) fn store_ldap_bind_password(
    realm: &str,
    password: &str,
    _domain_lock: &ApiLockGuard,
) -> Result<(), Error> {
    let mut data = proxmox_sys::fs::file_get_json(LDAP_PASSWORDS_FILENAME, Some(json!({})))?;
    data[realm] = password.into();
    let data = serde_json::to_vec_pretty(&data)?;

    proxmox_product_config::replace_secret_config(LDAP_PASSWORDS_FILENAME, &data)
}

/// Remove stored LDAP bind password. The domain config must be locked while this
/// function is executed.
pub(crate) fn remove_ldap_bind_password(
    realm: &str,
    _domain_lock: &ApiLockGuard,
) -> Result<(), Error> {
    let mut data = proxmox_sys::fs::file_get_json(LDAP_PASSWORDS_FILENAME, Some(json!({})))?;
    if let Some(map) = data.as_object_mut() {
        map.remove(realm);
    }
    let data = serde_json::to_vec_pretty(&data)?;

    proxmox_product_config::replace_secret_config(LDAP_PASSWORDS_FILENAME, &data)
}

/// Retrieve stored LDAP bind password
pub(super) fn get_ldap_bind_password(realm: &str) -> Result<Option<String>, Error> {
    let data = proxmox_sys::fs::file_get_json(LDAP_PASSWORDS_FILENAME, Some(json!({})))?;

    let password = data
        .get(realm)
        .and_then(|s| s.as_str())
        .map(|s| s.to_owned());

    Ok(password)
}
