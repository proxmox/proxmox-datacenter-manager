//! API types used during authentication.

use serde::{Deserialize, Serialize};

/// The JSON parameter object for the `/api2/access/ticket` API call.
///
/// Note that for Proxmox VE up to including version 7 the `new_format` parameter has to be used,
/// if TFA should be supported, as this crate does not support the old TFA login mechanism.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct CreateTicket {
    /// With webauthn the format of half-authenticated tickts changed. New
    /// clients should pass 1 here and not worry about the old format. The old
    /// format is deprecated and will be retired with PVE-8.0
    #[serde(deserialize_with = "crate::parse::deserialize_bool")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "new-format")]
    pub new_format: Option<bool>,

    /// One-time password for Two-factor authentication.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub otp: Option<String>,

    /// The secret password. This can also be a valid ticket.
    pub password: String,

    /// Verify ticket, and check if user have access 'privs' on 'path'
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Verify ticket, and check if user have access 'privs' on 'path'
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privs: Option<String>,

    /// You can optionally pass the realm using this parameter. Normally the
    /// realm is simply added to the username <username>@<relam>.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realm: Option<String>,

    /// The signed TFA challenge string the user wants to respond to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "tfa-challenge")]
    pub tfa_challenge: Option<String>,

    /// User name
    pub username: String,
}

/// The API response for a *complete* (both factors) `api2/access/ticket` call.
#[derive(Debug, Deserialize, Serialize)]
pub struct CreateTicketResponse {
    /// The CSRF prevention token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "CSRFPreventionToken")]
    pub csrfprevention_token: Option<String>,

    /// The cluster's visual name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clustername: Option<String>,

    /// The ticket as is supposed to be used in the authentication header.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<String>,

    /// The full userid with the `@realm` part.
    pub username: String,
}

#[derive(Deserialize)]
pub struct ApiResponse<T> {
    pub data: Option<T>,
}
