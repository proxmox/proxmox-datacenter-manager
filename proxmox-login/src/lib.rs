//! This package provides helpers for logging into the APIs of Proxmox products such as Proxmox VE
//! or Proxmox Backup.

use serde::{Deserialize, Serialize};

pub mod parse;

pub mod api;
pub mod error;
pub mod tfa;
pub mod ticket;

const METHOD_POST: &str = "POST";
const CONTENT_TYPE_JSON: &str = "application/json";

#[doc(inline)]
pub use ticket::{Authentication, Ticket};

use error::{ResponseError, TfaError, TicketError};

/// The header name for the CSRF prevention token.
pub const CSRF_HEADER_NAME: &str = "CSRFPreventionToken";

/// A request to be sent to the ticket API call.
///
/// Note that the body is always JSON.
#[derive(Clone, Debug)]
pub struct Request {
    /// This is always 'POST'.
    pub method: &'static str,

    pub url: String,

    /// This is always `application/json`.
    pub content_type: &'static str,

    /// The `Content-length` header field.
    pub content_length: usize,

    /// The body.
    pub body: Vec<u8>,
}

/// Login or ticket renewal request builder.
///
/// This takes an API URL and either a valid ticket or a userid (name + real) and password in order
/// to create an HTTP [`Request`] to renew or create a new API ticket.
///
/// Note that for Proxmox VE versions up to including 7, a compatibility flag is required to
/// support Two-Factor-Authentication.
#[derive(Debug)]
pub struct Login {
    api_url: String,
    userid: String,
    password: String,
    pve_compat: bool,
}

fn normalize_url(mut api_url: String) -> String {
    api_url.truncate(api_url.trim_end_matches('/').len());
    api_url
}

impl Login {
    /// Prepare a request given an existing ticket string.
    pub fn renew(api_url: String, ticket: String) -> Result<Self, TicketError> {
        Ok(Self::renew_ticket(api_url, ticket.parse()?))
    }

    /// Switch to a different url on the same server.
    pub fn set_url(&mut self, api_url: String) {
        self.api_url = api_url;
    }

    /// Prepare a request given an already parsed ticket.
    pub fn renew_ticket(api_url: String, ticket: Ticket) -> Self {
        Self {
            api_url: normalize_url(api_url),
            pve_compat: ticket.product() == "PVE",
            userid: ticket.userid().to_string(),
            password: ticket.into(),
        }
    }

    /// Prepare a request given a userid and password.
    pub fn new(api_url: String, userid: String, password: String) -> Self {
        Self {
            api_url: normalize_url(api_url),
            userid,
            password,
            pve_compat: false,
        }
    }

    /// Set the Proxmox VE compatibility parameter for Two-Factor-Authentication support.
    pub fn pve_compatibility(mut self, compatibility: bool) -> Self {
        self.pve_compat = compatibility;
        self
    }

    /// Create an HTTP [`Request`] from the current data.
    ///
    /// If the request returns a successful result, the response's body should be passed to the
    /// [`response`](Login::response) method in order to extract the validated ticket or
    /// Two-Factor-Authentication challenge.
    pub fn request(&self) -> Result<Request, serde_json::Error> {
        let request = api::CreateTicket {
            new_format: self.pve_compat.then_some(true),
            username: self.userid.clone(),
            password: self.password.clone(),
            ..Default::default()
        };

        let body = serde_json::to_string(&request)?.into_bytes();

        Ok(Request {
            method: METHOD_POST,
            url: format!("{}/api2/json/access/ticket", self.api_url),
            content_type: CONTENT_TYPE_JSON,
            content_length: body.len(),
            body,
        })
    }

    /// Parse the result body of a [`CreateTicket`](api::CreateTicket) API request.
    ///
    /// On success, this will either yield an [`Authentication`] or a [`SecondFactorChallenge`] if
    /// Two-Factor-Authentication is required.
    pub fn response(&self, body: &[u8]) -> Result<TicketResult, ResponseError> {
        use ticket::TicketResponse;

        let response: api::ApiResponse<api::CreateTicketResponse> = serde_json::from_slice(body)?;
        let response = response.data.ok_or("missing response data")?;

        if response.username != self.userid {
            return Err("ticket response contained unexpected userid".into());
        }

        let ticket: TicketResponse = match response.ticket {
            Some(ticket) => ticket.parse()?,
            None => return Err("missing ticket".into()),
        };

        Ok(match ticket {
            TicketResponse::Full(ticket) => {
                if ticket.userid() != self.userid {
                    return Err("returned ticket contained unexpected userid".into());
                }
                TicketResult::Full(Authentication {
                    csrfprevention_token: response
                        .csrfprevention_token
                        .ok_or("missing CSRFPreventionToken in ticket response")?,
                    clustername: response.clustername,
                    api_url: self.api_url.clone(),
                    userid: response.username,
                    ticket,
                })
            }

            TicketResponse::Tfa(ticket, challenge) => {
                TicketResult::TfaRequired(SecondFactorChallenge {
                    api_url: self.api_url.clone(),
                    pve_compat: self.pve_compat,
                    userid: response.username,
                    ticket,
                    challenge,
                })
            }
        })
    }
}

/// This is the result of a ticket call. It will either yield a final ticket, or a TFA challenge.
///
/// This is serializable in order to easily store it for later reuse.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TicketResult {
    /// The response contained a valid ticket.
    Full(Authentication),

    /// The response returned a Two-Factor-Authentication challenge.
    TfaRequired(SecondFactorChallenge),
}

/// A ticket call can returned a TFA challenge. The user should inspect the
/// [`challenge`](tfa::TfaChallenge) member and call one of the `respond_*` methods which will
/// yield a HTTP [`Request`] which should be used to finish the authentication.
///
/// Finally, the response should be passed to the [`response`](SecondFactorChallenge::response)
/// method to get the ticket.
///
/// This is serializable in order to easily store it for later reuse.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SecondFactorChallenge {
    api_url: String,
    pve_compat: bool,
    userid: String,
    ticket: String,
    pub challenge: tfa::TfaChallenge,
}

impl SecondFactorChallenge {
    /// Create a HTTP request responding to a Yubico OTP challenge.
    ///
    /// Errors with `TfaError::Unavailable` if Yubic OTP is not available.
    pub fn respond_yubico(&self, code: &str) -> Result<Request, TfaError> {
        if !self.challenge.yubico {
            Err(TfaError::Unavailable)
        } else {
            self.respond_raw(&format!("yubico:{code}"))
        }
    }

    /// Create a HTTP request responding with a TOTP value.
    ///
    /// Errors with `TfaError::Unavailable` if TOTP is not available.
    pub fn respond_totp(&self, code: &str) -> Result<Request, TfaError> {
        if !self.challenge.totp {
            Err(TfaError::Unavailable)
        } else {
            self.respond_raw(&format!("totp:{code}"))
        }
    }

    /// Create a HTTP request responding with a recovery code.
    ///
    /// Errors with `TfaError::Unavailable` if no recovery codes are available.
    pub fn respond_recovery(&self, code: &str) -> Result<Request, TfaError> {
        if !self.challenge.recovery.is_available() {
            Err(TfaError::Unavailable)
        } else {
            self.respond_raw(&format!("recovery:{code}"))
        }
    }

    #[cfg(feature = "webauthn")]
    /// Create a HTTP request responding with a FIDO2/webauthn result JSON string.
    ///
    /// Errors with `TfaError::Unavailable` if no webauthn challenge was available.
    pub fn respond_webauthn(&self, json_string: &str) -> Result<Request, TfaError> {
        if self.challenge.webauthn.is_none() {
            Err(TfaError::Unavailable)
        } else {
            self.respond_raw(&format!("webauthn:{json_string}"))
        }
    }

    /// Create a HTTP request using a raw response.
    ///
    /// A raw response is the response string prefixed with its challenge type and a colon.
    pub fn respond_raw(&self, data: &str) -> Result<Request, TfaError> {
        let request = api::CreateTicket {
            new_format: self.pve_compat.then_some(true),
            username: self.userid.clone(),
            password: data.to_string(),
            tfa_challenge: Some(self.ticket.clone()),
            ..Default::default()
        };

        let body = serde_json::to_string(&request)?.into_bytes();

        Ok(Request {
            method: METHOD_POST,
            url: format!("{}/api2/json/access/ticket", self.api_url),
            content_type: CONTENT_TYPE_JSON,
            content_length: body.len(),
            body,
        })
    }

    /// Deal with the API's response object to extract the ticket.
    pub fn response(&self, body: &[u8]) -> Result<Authentication, ResponseError> {
        let response: api::ApiResponse<api::CreateTicketResponse> = serde_json::from_slice(body)?;
        let response = response.data.ok_or("missing response data")?;

        if response.username != self.userid {
            return Err("ticket response contained unexpected userid".into());
        }

        let ticket: Ticket = response.ticket.ok_or("no ticket in response")?.parse()?;

        if ticket.userid() != self.userid {
            return Err("returned ticket contained unexpected userid".into());
        }

        Ok(Authentication {
            ticket,
            csrfprevention_token: response
                .csrfprevention_token
                .ok_or("missing CSRFPreventionToken in ticket response")?,
            clustername: response.clustername,
            userid: response.username,
            api_url: self.api_url.clone(),
        })
    }
}
