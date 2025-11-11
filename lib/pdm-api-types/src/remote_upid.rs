use std::fmt;

use anyhow::{bail, Error};

use proxmox_schema::api_types::SAFE_ID_REGEX;
use proxmox_schema::{ApiType, Schema, StringSchema};

pub const REMOTE_UPID_SCHEMA: Schema = StringSchema::new("A remote UPID")
    .min_length("C!UPID:N:12345678:12345678:12345678:::".len())
    .schema();

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct RemoteUpid {
    remote: String,
    /// This is usually a pve upid, but may also be a pbs upid, they have distinct formats.
    upid: String,
}

impl RemoteUpid {
    pub fn remote(&self) -> &str {
        &self.remote
    }

    pub fn into_remote(self) -> String {
        self.remote
    }

    /// Get the raw UPID.
    pub fn upid(&self) -> &str {
        &self.upid
    }

    /// Get the raw UPID, consuming self.
    pub fn into_upid(self) -> String {
        self.upid
    }
}

impl ApiType for RemoteUpid {
    const API_SCHEMA: Schema = REMOTE_UPID_SCHEMA;
}

impl TryFrom<(String, String)> for RemoteUpid {
    type Error = Error;

    fn try_from((remote, upid): (String, String)) -> Result<Self, Error> {
        if !SAFE_ID_REGEX.is_match(&remote) {
            bail!("bad remote id in remote upid");
        }

        Ok(Self { remote, upid })
    }
}

impl TryFrom<(String, &str)> for RemoteUpid {
    type Error = Error;

    fn try_from((remote, upid): (String, &str)) -> Result<Self, Error> {
        if !SAFE_ID_REGEX.is_match(&remote) {
            bail!("bad remote id in remote upid");
        }

        Ok(Self {
            remote,
            upid: upid.to_string(),
        })
    }
}

impl TryFrom<(&str, &str)> for RemoteUpid {
    type Error = Error;

    fn try_from((remote, upid): (&str, &str)) -> Result<Self, Error> {
        if !SAFE_ID_REGEX.is_match(remote) {
            bail!("bad remote id in remote upid");
        }

        Ok(Self {
            remote: remote.to_string(),
            upid: upid.to_string(),
        })
    }
}

impl std::str::FromStr for RemoteUpid {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        match s.find('!') {
            None => bail!("missing '!' separator in remote upid"),
            Some(pos) => (&s[..pos], &s[(pos + 1)..]).try_into(),
        }
    }
}

impl fmt::Display for RemoteUpid {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}!{}", self.remote, self.upid)
    }
}

serde_plain::derive_deserialize_from_fromstr!(RemoteUpid, "valid remote upid");
serde_plain::derive_serialize_from_display!(RemoteUpid);
