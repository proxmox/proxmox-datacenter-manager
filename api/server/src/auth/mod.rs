//! Provides authentication primitives for the HTTP server

use std::future::Future;
use std::net::IpAddr;
use std::pin::Pin;

use anyhow::{bail, Error};
use once_cell::sync::OnceCell;

use proxmox_auth_api::api::{Authenticator, LockedTfaConfig};
use proxmox_auth_api::types::Authid;
use proxmox_auth_api::{HMACKey, Keyring};
use proxmox_rest_server::AuthError;
use proxmox_router::UserInformation;
use proxmox_tfa::api::{OpenUserChallengeData, TfaConfig};

use pdm_api_types::{RealmRef, Userid};
use pdm_config::CachedUserInfo;

pub mod certs;
pub mod csrf;
pub mod key;
pub mod tfa;

/// Pre-load lazy-static pre-load things like csrf & auth key
pub fn init(use_private_key: bool) {
    let _ = key::public_auth_key(); // load with lazy_static
    let _ = csrf::csrf_secret(); // load with lazy_static
    setup_auth_context(use_private_key);
}

pub fn setup_keys() -> Result<(), Error> {
    if let Err(err) = key::generate_auth_key() {
        bail!("unable to generate auth key - {err}");
    }
    if let Err(err) = csrf::generate_csrf_key() {
        bail!("unable to generate csrf key - {err}");
    }
    if let Err(err) = certs::update_self_signed_cert(false) {
        bail!("unable to generate TLS certs - {err}");
    }
    Ok(())
}

pub async fn check_auth(
    headers: &http::HeaderMap,
    method: &hyper::Method,
) -> Result<(String, Box<dyn UserInformation + Sync + Send>), AuthError> {
    let user_info = CachedUserInfo::new()?;

    proxmox_auth_api::api::http_check_auth(headers, method)
        .map(move |name| (name, Box::new(user_info) as _))
}

static AUTH_CONTEXT: OnceCell<PdmAuthContext> = OnceCell::new();

fn setup_auth_context(use_private_key: bool) {
    let keyring = if use_private_key {
        Keyring::with_private_key(key::private_auth_key().clone())
    } else {
        Keyring::with_public_key(key::public_auth_key().clone())
    };

    AUTH_CONTEXT
        .set(PdmAuthContext {
            keyring,
            csrf_secret: csrf::csrf_secret(),
        })
        .map_err(drop)
        .expect("auth context setup twice");

    proxmox_auth_api::set_auth_context(AUTH_CONTEXT.get().unwrap());
}

struct PdmAuthContext {
    keyring: Keyring,
    csrf_secret: &'static HMACKey,
}

impl proxmox_auth_api::api::AuthContext for PdmAuthContext {
    fn lookup_realm(&self, realm: &RealmRef) -> Option<Box<dyn Authenticator + Send + Sync>> {
        lookup_authenticator(realm).ok()
    }

    /// Get the current authentication keyring.
    fn keyring(&self) -> &Keyring {
        &self.keyring
    }

    /// The auth prefix without the separating colon. Eg. `"PDM"`.
    fn auth_prefix(&self) -> &'static str {
        "PDM"
    }

    /// API token prefix (without the `'='`).
    fn auth_token_prefix(&self) -> &'static str {
        "PDMAPIToken"
    }

    /// Auth cookie name.
    fn auth_cookie_name(&self) -> &'static str {
        "PDMAuthCookie"
    }

    /// Check if a userid is enabled and return a [`UserInformation`] handle.
    fn auth_id_is_active(&self, auth_id: &Authid) -> Result<bool, Error> {
        Ok(pdm_config::CachedUserInfo::new()?.is_active_auth_id(auth_id))
    }

    /// Access the TFA config with an exclusive lock.
    fn tfa_config_write_lock(&self) -> Result<Box<dyn LockedTfaConfig>, Error> {
        Ok(Box::new(PdmLockedTfaConfig {
            _lock: tfa::read_lock()?,
            config: tfa::read()?,
        }))
    }

    /// CSRF prevention token secret data.
    fn csrf_secret(&self) -> &'static HMACKey {
        &self.csrf_secret
    }

    /// Verify a token secret.
    fn verify_token_secret(&self, token_id: &Authid, token_secret: &str) -> Result<(), Error> {
        pdm_config::token_shadow::verify_secret(token_id, token_secret)
    }

    // /// Check path based tickets. (Used for terminal tickets).
    //fn check_path_ticket(
    //    &self,
    //    userid: &Userid,
    //    password: &str,
    //    path: String,
    //    privs: String,
    //    port: u16,
    //) -> Result<Option<bool>, Error> {
    //    Ok(None)
    //}
}

pub(crate) fn lookup_authenticator(
    realm: &RealmRef,
) -> Result<Box<dyn Authenticator + Send + Sync>, Error> {
    match realm.as_str() {
        "pam" => Ok(Box::new(proxmox_auth_api::Pam::new(
            "proxmox-datacenter-auth",
        ))),
        // "pdm" => Ok(Box::new(PdmAuthenticator)),
        realm => bail!("unknown realm '{}'", realm),
    }
}

/// Authenticate users
pub(crate) fn authenticate_user<'a>(
    userid: &'a Userid,
    password: &'a str,
    client_ip: Option<&'a IpAddr>,
) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + 'a>> {
    Box::pin(async move {
        lookup_authenticator(userid.realm())?
            .authenticate_user(userid.name(), password, client_ip)
            .await?;
        Ok(())
    })
}

struct PdmLockedTfaConfig {
    _lock: proxmox_product_config::ApiLockGuard,
    config: TfaConfig,
}

static USER_ACCESS: tfa::UserAccess = tfa::UserAccess;

impl LockedTfaConfig for PdmLockedTfaConfig {
    fn config_mut(&mut self) -> (&dyn OpenUserChallengeData, &mut TfaConfig) {
        (&USER_ACCESS, &mut self.config)
    }

    fn save_config(&mut self) -> Result<(), Error> {
        tfa::write(&self.config)
    }
}
