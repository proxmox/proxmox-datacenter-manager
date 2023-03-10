//! Ticket related data.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::TicketError;
use crate::tfa::TfaChallenge;

/// The repsonse to a ticket call can either be a complete ticket, or a TFA challenge.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) enum TicketResponse {
    Full(Ticket),
    Tfa(String, TfaChallenge),
}

impl std::str::FromStr for TicketResponse {
    type Err = TicketError;

    fn from_str(ticket: &str) -> Result<Self, TicketError> {
        let pos = ticket.find(':').ok_or(TicketError)?;
        match ticket[pos..].strip_prefix(":!tfa!") {
            Some(challenge) => match challenge.find(':') {
                Some(pos) => {
                    let challenge: std::borrow::Cow<[u8]> =
                        percent_encoding::percent_decode_str(&challenge[..pos]).into();
                    let challenge = serde_json::from_slice(&challenge).map_err(|_| TicketError)?;
                    Ok(TicketResponse::Tfa(ticket.to_string(), challenge))
                }
                None => Err(TicketError),
            },
            None => ticket.parse().map(TicketResponse::Full),
        }
    }
}

/// An API ticket string. Serializable so it can be stored for later reuse.
#[derive(Clone, Debug)]
pub struct Ticket {
    data: Box<str>,
    timestamp: i64,
    product_len: u16,
    userid_len: u16,
    // timestamp_len: u16,
}

/// Tickets are valid for 2 hours.
const TICKET_LIFETIME: i64 = 2 * 3600;
/// We refresh during the last half hour.
const REFRESH_EARLY_BY: i64 = 1800;

impl Ticket {
    /// The ticket's product prefix.
    pub fn product(&self) -> &str {
        &self.data[..usize::from(self.product_len)]
    }

    /// The userid contained in the ticket.
    pub fn userid(&self) -> &str {
        let start = usize::from(self.product_len) + 1;
        let len = usize::from(self.userid_len);
        &self.data[start..(start + len)]
    }

    /// Thet ticket's timestamp as a UNIX epoch.
    pub fn timestamp(&self) -> i64 {
        self.timestamp
    }

    /// The ticket age in seconds.
    pub fn age(&self) -> i64 {
        epoch_i64() - self.timestamp
    }

    /// This is a convenience check for the ticket's validity assuming the usual ticket lifetime of
    /// 2 hours.
    pub fn validity(&self) -> Validity {
        let age = self.age();
        if age > TICKET_LIFETIME {
            Validity::Expired
        } else if age >= TICKET_LIFETIME - REFRESH_EARLY_BY {
            Validity::Refresh
        } else {
            Validity::Valid
        }
    }

    /// Get the cookie in the form `<PRODUCT>AuthCookie=Ticket`.
    pub fn cookie(&self) -> String {
        format!("{}AuthCookie={}", self.product(), self.data)
    }
}

/// Whether a ticket should be refreshed or is already invalid and needs to be completely renewed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Validity {
    /// The ticket is still valid for longer than half an hour.
    Valid,

    /// The ticket is within its final half hour validity period and should be renewed with the
    /// ticket as password.
    Refresh,

    /// The ticket is already invalid and a new ticket needs to be created.
    Expired,
}

impl Validity {
    /// Simply check whether the ticket is considered valid even if it should be renewed.
    pub fn is_valid(self) -> bool {
        matches!(self, Validity::Valid | Validity::Refresh)
    }
}

impl std::str::FromStr for Ticket {
    type Err = TicketError;

    fn from_str(s: &str) -> Result<Self, TicketError> {
        let data = s;

        // get product:
        let product_len = s.find(':').ok_or(TicketError)?;
        if product_len >= 10 {
            // weird product
            return Err(TicketError);
        }
        let s = &s[(product_len + 1)..];

        // get userid:
        let userid_len = s.find(':').ok_or(TicketError)?;
        if !s[..userid_len].contains('@') {
            return Err(TicketError);
        }
        let s = &s[(userid_len + 1)..];

        // timestamp
        let timestamp_len = s.find(':').ok_or(TicketError)?;
        let timestamp = i64::from_str_radix(&s[..timestamp_len], 16).map_err(|_| TicketError)?;

        let s = &s[(timestamp_len + 1)..];

        let s = s.strip_prefix(':').ok_or(TicketError)?;
        if s.is_empty() {
            return Err(TicketError);
        }

        Ok(Self {
            product_len: u16::try_from(product_len).map_err(|_| TicketError)?,
            userid_len: u16::try_from(userid_len).map_err(|_| TicketError)?,
            //timestamp_len: u16::try_from(timestamp_len).map_err(|_| TicketError)?,
            timestamp,
            data: data.into(),
        })
    }
}

impl fmt::Display for Ticket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.data)
    }
}

impl From<Ticket> for String {
    fn from(ticket: Ticket) -> String {
        ticket.data.into()
    }
}

impl From<Ticket> for Box<str> {
    fn from(ticket: Ticket) -> Box<str> {
        ticket.data
    }
}

impl Serialize for Ticket {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.data)
    }
}

impl<'de> Deserialize<'de> for Ticket {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        std::borrow::Cow::<'de, str>::deserialize(deserializer)?
            .parse()
            .map_err(D::Error::custom)
    }
}

/// A finished authentication state.
///
/// This is serializable / deserializable in order to be able to easily store it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Authentication {
    /// The API URL this authentication info belongs to.
    pub api_url: String,

    /// The user id in the form of `username@realm`.
    pub userid: String,

    /// The authentication ticket.
    pub ticket: Ticket,

    /// The cluster name (if any)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clustername: Option<String>,

    /// The CSRFPreventionToken header.
    #[serde(rename = "CSRFPreventionToken")]
    pub csrfprevention_token: String,
}

impl Authentication {
    /// Get the ticket cookie in the form `<PRODUCT>AuthCookie=Ticket`.
    pub fn cookie(&self) -> String {
        self.ticket.cookie()
    }

    #[cfg(feature = "http")]
    /// Add authentication headers to a request.
    ///
    /// This is equivalent to doing:
    /// ```ignore
    /// request
    ///     .header(http::header::COOKIE, auth.cookie())
    ///     .header(proxmox_login::CSRF_HEADER_NAME, &auth.csrfprevention_token)
    /// ```
    pub fn set_auth_headers(&self, request: http::request::Builder) -> http::request::Builder {
        request
            .header(http::header::COOKIE, self.cookie())
            .header(crate::CSRF_HEADER_NAME, &self.csrfprevention_token)
    }
}

fn epoch_i64() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now();
    if now > UNIX_EPOCH {
        i64::try_from(now.duration_since(UNIX_EPOCH).unwrap().as_secs()).unwrap_or(0)
    } else {
        -i64::try_from(UNIX_EPOCH.duration_since(now).unwrap().as_secs()).unwrap_or(0)
    }
}
