//! Provides authentication primitives for the HTTP server

pub mod certs;
pub mod csrf;
pub mod key;
pub mod tfa;
pub mod ticket;

use anyhow::format_err;

use proxmox_router::UserInformation;

use pdm_api_types::{Authid, Userid};
use pdm_config::{token_shadow, CachedUserInfo};
use pdm_ticket::Ticket;
use proxmox_rest_server::{extract_cookie, AuthError};

use hyper::header;
use percent_encoding::percent_decode_str;

/// Pre-load lazy-static pre-load things like csrf & auth key
pub fn init() {
    let _ = key::public_auth_key(); // load with lazy_static
    let _ = csrf::csrf_secret(); // load with lazy_static
}

struct UserAuthData {
    ticket: String,
    csrf_token: Option<String>,
}

enum AuthData {
    User(UserAuthData),
    ApiToken(String),
}

fn extract_auth_data(headers: &http::HeaderMap) -> Option<AuthData> {
    if let Some(raw_cookie) = headers.get(header::COOKIE) {
        if let Ok(cookie) = raw_cookie.to_str() {
            if let Some(ticket) = extract_cookie(cookie, "PDMAuthCookie") {
                let csrf_token = match headers.get("CSRFPreventionToken").map(|v| v.to_str()) {
                    Some(Ok(v)) => Some(v.to_owned()),
                    _ => None,
                };
                return Some(AuthData::User(UserAuthData { ticket, csrf_token }));
            }
        }
    }

    match headers.get(header::AUTHORIZATION).map(|v| v.to_str()) {
        Some(Ok(v)) => {
            if v.starts_with("PDMAPIToken ") || v.starts_with("PDMAPIToken=") {
                Some(AuthData::ApiToken(v["PDMAPIToken ".len()..].to_owned()))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub async fn check_auth(
    headers: &http::HeaderMap,
    method: &hyper::Method,
) -> Result<(String, Box<dyn UserInformation + Sync + Send>), AuthError> {
    // fixme: make all IO async

    let user_info = CachedUserInfo::new()?;

    let auth_data = extract_auth_data(headers);
    match auth_data {
        Some(AuthData::User(user_auth_data)) => {
            let ticket = user_auth_data.ticket.clone();
            let ticket_lifetime = pdm_ticket::TICKET_LIFETIME;

            let userid: Userid = Ticket::<ticket::ApiTicket>::parse(&ticket)?
                .verify_with_time_frame(key::public_auth_key(), "PBS", None, -300..ticket_lifetime)?
                .require_full()?;

            let auth_id = Authid::from(userid.clone());
            if !user_info.is_active_auth_id(&auth_id) {
                return Err(format_err!("user account disabled or expired.").into());
            }

            if method != hyper::Method::GET {
                if let Some(csrf_token) = &user_auth_data.csrf_token {
                    csrf::verify_csrf_prevention_token(&userid, csrf_token, -300, ticket_lifetime)?;
                } else {
                    return Err(format_err!("missing CSRF prevention token").into());
                }
            }

            Ok((auth_id.to_string(), Box::new(user_info)))
        }
        Some(AuthData::ApiToken(api_token)) => {
            let mut parts = api_token.splitn(2, ':');
            let tokenid = parts
                .next()
                .ok_or_else(|| format_err!("failed to split API token header"))?;
            let tokenid: Authid = tokenid.parse()?;

            if !user_info.is_active_auth_id(&tokenid) {
                return Err(format_err!("user account or token disabled or expired.").into());
            }

            let tokensecret = parts
                .next()
                .ok_or_else(|| format_err!("failed to split API token header"))?;
            let tokensecret = percent_decode_str(tokensecret)
                .decode_utf8()
                .map_err(|_| format_err!("failed to decode API token header"))?;

            token_shadow::verify_secret(&tokenid, &tokensecret)?;

            Ok((tokenid.to_string(), Box::new(user_info)))
        }
        None => Err(AuthError::NoData),
    }
}
