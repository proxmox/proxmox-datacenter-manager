use std::fmt;

use anyhow::{bail, Error};

use proxmox_schema::api_types::SAFE_ID_REGEX;
use proxmox_schema::{ApiType, Schema, StringSchema};

use crate::remotes::RemoteType;

pub const REMOTE_UPID_SCHEMA: Schema = StringSchema::new("A remote UPID")
    .min_length("abc:C!UPID:N:12345678:12345678:12345678:::".len())
    .schema();

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
/// A UPID type for tasks on a specific remote.
pub struct RemoteUpid {
    remote: String,
    remote_type: RemoteType,
    // This can either be a PVE UPID or a PBS UPID, both have distinct, incompatible formats.
    upid: String,
}

/// Type containing the parsed, native UPID for each type of remote.
pub enum NativeUpid {
    PveUpid(pve_api_types::PveUpid),
    PbsUpid(pbs_api_types::UPID),
}

impl RemoteUpid {
    /// Create a new remote UPID.
    pub fn new(remote: String, remote_type: RemoteType, upid: String) -> Self {
        Self {
            remote,
            upid,
            remote_type,
        }
    }
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

    /// Return the type of the remote which corresponds to this UPID.
    pub fn remote_type(&self) -> RemoteType {
        self.remote_type
    }

    /// Get the parsed, native UPID type.
    ///
    /// This function will return an error if the UPID could not be parsed.
    pub fn native_upid(&self) -> Result<NativeUpid, Error> {
        Ok(match self.remote_type() {
            RemoteType::Pve => NativeUpid::PveUpid(self.upid.parse()?),
            RemoteType::Pbs => NativeUpid::PbsUpid(self.upid.parse()?),
        })
    }

    /// Get the parsed PVE UPID.
    ///
    /// If the UPID could not be parsed, or has an unexpected format (PBS),
    /// an error is returned.
    pub fn pve_upid(&self) -> Result<pve_api_types::PveUpid, Error> {
        match self.native_upid()? {
            NativeUpid::PveUpid(pve_upid) => Ok(pve_upid),
            NativeUpid::PbsUpid(_) => bail!("got a PBS UPID when expecting a PVE UPID"),
        }
    }

    /// Get the parsed PBS UPID.
    ///
    /// If the UPID could not be parsed, or has an unexpected format (PVE),
    /// an error is returned.
    pub fn pbs_upid(&self) -> Result<pbs_api_types::UPID, Error> {
        match self.native_upid()? {
            NativeUpid::PveUpid(_) => bail!("got a PVE UPID when expecting a PBS UPID"),
            NativeUpid::PbsUpid(pbs_upid) => Ok(pbs_upid),
        }
    }

    fn deduce_type(raw_upid: &str) -> Result<RemoteType, Error> {
        if raw_upid.parse::<pve_api_types::PveUpid>().is_ok() {
            Ok(RemoteType::Pve)
        } else if raw_upid.parse::<pbs_api_types::UPID>().is_ok() {
            Ok(RemoteType::Pbs)
        } else {
            bail!("invalid upid: {raw_upid}");
        }
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

        let ty = Self::deduce_type(&upid)?;

        Ok(Self {
            remote,
            upid,
            remote_type: ty,
        })
    }
}

impl TryFrom<(String, &str)> for RemoteUpid {
    type Error = Error;

    fn try_from((remote, upid): (String, &str)) -> Result<Self, Error> {
        if !SAFE_ID_REGEX.is_match(&remote) {
            bail!("bad remote id in remote upid");
        }

        let ty = Self::deduce_type(upid)?;

        Ok(Self {
            remote,
            upid: upid.to_string(),
            remote_type: ty,
        })
    }
}

impl TryFrom<(&str, &str)> for RemoteUpid {
    type Error = Error;

    fn try_from((remote, upid): (&str, &str)) -> Result<Self, Error> {
        if !SAFE_ID_REGEX.is_match(remote) {
            bail!("bad remote id in remote upid");
        }

        let ty = Self::deduce_type(upid)?;

        Ok(Self {
            remote: remote.to_string(),
            upid: upid.to_string(),
            remote_type: ty,
        })
    }
}

impl TryFrom<(&str, &str, &str)> for RemoteUpid {
    type Error = Error;

    fn try_from((ty, remote, upid): (&str, &str, &str)) -> Result<Self, Error> {
        if !SAFE_ID_REGEX.is_match(remote) {
            bail!("bad remote id in remote upid");
        }

        let ty = ty.parse()?;

        Ok(Self {
            remote: remote.to_string(),
            upid: upid.to_string(),
            remote_type: ty,
        })
    }
}

impl std::str::FromStr for RemoteUpid {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        match s.split_once('!') {
            None => bail!("missing '!' separator in remote upid"),
            Some((remote_and_type, upid)) => match remote_and_type.split_once(':') {
                Some((ty, remote)) => (ty, remote, upid).try_into(),
                None => (remote_and_type, upid).try_into(),
            },
        }
    }
}

impl fmt::Display for RemoteUpid {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}!{}", self.remote_type, self.remote, self.upid)
    }
}

serde_plain::derive_deserialize_from_fromstr!(RemoteUpid, "valid remote upid");
serde_plain::derive_serialize_from_display!(RemoteUpid);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str_old_format() {
        let pve_upid: RemoteUpid =
            "pve-remote!UPID:pve:00039E4D:002638B8:67B4A9D1:stopall::root@pam:"
                .parse()
                .unwrap();

        assert_eq!(pve_upid.remote(), "pve-remote");
        assert_eq!(pve_upid.remote_type(), RemoteType::Pve);
        assert_eq!(
            pve_upid.upid(),
            "UPID:pve:00039E4D:002638B8:67B4A9D1:stopall::root@pam:"
        );

        let pbs_upid: RemoteUpid =
            "pbs-remote!UPID:pbs:000002B2:00000158:00000000:674D828C:logrotate::root@pam:"
                .parse()
                .unwrap();

        assert_eq!(pbs_upid.remote(), "pbs-remote");
        assert_eq!(pbs_upid.remote_type(), RemoteType::Pbs);
        assert_eq!(
            pbs_upid.upid(),
            "UPID:pbs:000002B2:00000158:00000000:674D828C:logrotate::root@pam:"
        );
    }

    #[test]
    fn test_from_str_new_format() {
        let pve_upid: RemoteUpid =
            "pve:pve-remote!UPID:pve:00039E4D:002638B8:67B4A9D1:stopall::root@pam:"
                .parse()
                .unwrap();

        assert_eq!(pve_upid.remote(), "pve-remote");
        assert_eq!(pve_upid.remote_type(), RemoteType::Pve);
        assert_eq!(
            pve_upid.upid(),
            "UPID:pve:00039E4D:002638B8:67B4A9D1:stopall::root@pam:"
        );

        let pbs_upid: RemoteUpid =
            "pbs:pbs-remote!UPID:pbs:000002B2:00000158:00000000:674D828C:logrotate::root@pam:"
                .parse()
                .unwrap();

        assert_eq!(pbs_upid.remote(), "pbs-remote");
        assert_eq!(pbs_upid.remote_type(), RemoteType::Pbs);
        assert_eq!(
            pbs_upid.upid(),
            "UPID:pbs:000002B2:00000158:00000000:674D828C:logrotate::root@pam:"
        );
    }

    #[test]
    fn test_display() {
        let pve_upid = RemoteUpid::new(
            "pve-remote".to_string(),
            RemoteType::Pve,
            "UPID:pve:00039E4D:002638B8:67B4A9D1:stopall::root@pam:".to_string(),
        );

        assert_eq!(
            pve_upid.to_string(),
            "pve:pve-remote!UPID:pve:00039E4D:002638B8:67B4A9D1:stopall::root@pam:"
        );

        let pbs_upid = RemoteUpid::new(
            "pbs-remote".to_string(),
            RemoteType::Pbs,
            "UPID:pbs:000002B2:00000158:00000000:674D828C:logrotate::root@pam:".to_string(),
        );

        assert_eq!(
            pbs_upid.to_string(),
            "pbs:pbs-remote!UPID:pbs:000002B2:00000158:00000000:674D828C:logrotate::root@pam:"
        );
    }
}
