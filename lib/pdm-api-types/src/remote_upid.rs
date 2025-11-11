use std::fmt;

use anyhow::{bail, Error};

use proxmox_schema::api_types::SAFE_ID_REGEX;
use proxmox_schema::{ApiType, Schema, StringSchema};

pub const REMOTE_UPID_SCHEMA: Schema = StringSchema::new("A remote UPID")
    .min_length("C!UPID:N:12345678:12345678:12345678:::".len())
    .schema();

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
/// A UPID type for tasks on a specific remote.
pub struct RemoteUpid {
    remote: String,
    // This can either be a PVE UPID or a PBS UPID, both have distinct, incompatible formats.
    upid: String,
}

impl RemoteUpid {
    /// Get the remote for this UPID.
    pub fn remote(&self) -> &str {
        &self.remote
    }

    /// Get the remote for this UPID, consuming self.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str() {
        let pve_upid: RemoteUpid =
            "pve-remote!UPID:pve:00039E4D:002638B8:67B4A9D1:stopall::root@pam:"
                .parse()
                .unwrap();

        assert_eq!(pve_upid.remote(), "pve-remote");
        assert_eq!(
            pve_upid.upid(),
            "UPID:pve:00039E4D:002638B8:67B4A9D1:stopall::root@pam:"
        );

        let pbs_upid: RemoteUpid =
            "pbs-remote!UPID:pbs:000002B2:00000158:00000000:674D828C:logrotate::root@pam:"
                .parse()
                .unwrap();

        assert_eq!(pbs_upid.remote(), "pbs-remote");
        assert_eq!(
            pbs_upid.upid(),
            "UPID:pbs:000002B2:00000158:00000000:674D828C:logrotate::root@pam:"
        );
    }

    #[test]
    fn test_display() {
        let pve_upid = RemoteUpid {
            remote: "pve-remote".to_string(),
            upid: "UPID:pve:00039E4D:002638B8:67B4A9D1:stopall::root@pam:".to_string(),
        };

        assert_eq!(
            pve_upid.to_string(),
            "pve-remote!UPID:pve:00039E4D:002638B8:67B4A9D1:stopall::root@pam:"
        );

        let pbs_upid = RemoteUpid {
            remote: "pbs-remote".to_string(),
            upid: "UPID:pbs:000002B2:00000158:00000000:674D828C:logrotate::root@pam:".to_string(),
        };

        assert_eq!(
            pbs_upid.to_string(),
            "pbs-remote!UPID:pbs:000002B2:00000158:00000000:674D828C:logrotate::root@pam:"
        );
    }
}
