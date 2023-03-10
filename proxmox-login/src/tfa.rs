//! These types are from the `proxmox-tfa` crate. Currently the 'api' feature is required for this,
//! but we should add a feature that exposes the types without the api implementation and drop the
//! types from here.

use std::fmt;

use serde::{Deserialize, Serialize};

/// When sending a TFA challenge to the user, we include information about what kind of challenge
/// the user may perform. If webauthn credentials are available, a webauthn challenge will be
/// included.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TfaChallenge {
    /// True if the user has TOTP devices.
    #[serde(skip_serializing_if = "bool_is_false", default)]
    pub totp: bool,

    /// Whether there are recovery keys available.
    #[serde(skip_serializing_if = "RecoveryState::is_unavailable", default)]
    pub recovery: RecoveryState,

    #[cfg(feature = "webauthn")]
    /// If the user has any webauthn credentials registered, this will contain the corresponding
    /// challenge data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webauthn: Option<webauthn_rs::proto::RequestChallengeResponse>,

    /// True if the user has yubico keys configured.
    #[serde(skip_serializing_if = "bool_is_false", default)]
    pub yubico: bool,
}

fn bool_is_false(b: &bool) -> bool {
    !b
}

/// Used to inform the user about the recovery code status.
///
/// This contains the available key indices.
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct RecoveryState(Vec<usize>);

impl RecoveryState {
    pub fn is_available(&self) -> bool {
        !self.is_unavailable()
    }

    pub fn is_unavailable(&self) -> bool {
        self.0.is_empty()
    }
}

/// The "key" part of a registration, passed to `u2f.sign` in the registered keys list.
///
/// Part of the U2F API, therefore `camelCase` and base64url without padding.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisteredKey {
    /// Identifies the key handle on the client side. Used to create authentication challenges, so
    /// the client knows which key to use. Must be remembered.
    #[serde(with = "bytes_as_base64url_nopad")]
    pub key_handle: Vec<u8>,

    pub version: String,
}

mod bytes_as_base64url_nopad {
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        use serde::de::Error;
        String::deserialize(deserializer).and_then(|string| {
            base64::decode_config(&string, base64::URL_SAFE_NO_PAD)
                .map_err(|err| Error::custom(err.to_string()))
        })
    }
}

/// A user's response to a TFA challenge.
pub enum TfaResponse {
    Totp(String),
    U2f(serde_json::Value),
    Webauthn(serde_json::Value),
    Recovery(String),
}

#[derive(Debug)]
pub enum InvalidTfaResponse {
    Unknown,
    BadJson(serde_json::Error),
}

impl fmt::Display for InvalidTfaResponse {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            InvalidTfaResponse::Unknown => f.write_str("unrecognized tfa response type"),
            InvalidTfaResponse::BadJson(err) => fmt::Display::fmt(err, f),
        }
    }
}

impl std::error::Error for InvalidTfaResponse {}

impl From<serde_json::Error> for InvalidTfaResponse {
    fn from(err: serde_json::Error) -> Self {
        InvalidTfaResponse::BadJson(err)
    }
}

/// This is part of the REST API:
impl std::str::FromStr for TfaResponse {
    type Err = InvalidTfaResponse;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(if let Some(totp) = s.strip_prefix("totp:") {
            TfaResponse::Totp(totp.to_string())
        } else if let Some(u2f) = s.strip_prefix("u2f:") {
            TfaResponse::U2f(serde_json::from_str(u2f)?)
        } else if let Some(webauthn) = s.strip_prefix("webauthn:") {
            TfaResponse::Webauthn(serde_json::from_str(webauthn)?)
        } else if let Some(recovery) = s.strip_prefix("recovery:") {
            TfaResponse::Recovery(recovery.to_string())
        } else {
            return Err(InvalidTfaResponse::Unknown);
        })
    }
}
