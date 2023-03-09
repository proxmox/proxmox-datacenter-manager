//! Provides authentication primitives for the HTTP server

pub mod certs;
pub mod csrf;
pub mod key;
pub mod tfa;

use proxmox_router::UserInformation;

use pdm_config::CachedUserInfo;
use proxmox_rest_server::AuthError;

/// Pre-load lazy-static pre-load things like csrf & auth key
pub fn init() {
    let _ = key::public_auth_key(); // load with lazy_static
    let _ = csrf::csrf_secret(); // load with lazy_static
}

pub async fn check_auth(
    headers: &http::HeaderMap,
    method: &hyper::Method,
) -> Result<(String, Box<dyn UserInformation + Sync + Send>), AuthError> {
    let user_info = CachedUserInfo::new()?;

    proxmox_auth_api::api::http_check_auth(headers, method)
        .map(move |name| (name, Box::new(user_info) as _))
}
