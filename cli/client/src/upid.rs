use anyhow::{bail, Error};

use pdm_api_types::RemoteUpid;

pub fn parse_for_remote(remote: Option<&str>, upid: &str) -> Result<RemoteUpid, Error> {
    if upid.contains('!') {
        let upid: RemoteUpid = upid.parse()?;

        if let Some(remote) = remote {
            if upid.remote() != remote {
                bail!(
                    "remote in UPID ({:?}) does not match expected remote {remote:?}",
                    upid.remote()
                );
            }
        }

        Ok(upid)
    } else {
        match remote {
            Some(remote) => format!("{remote}!{upid}").parse(),
            None => bail!("upid without a remote"),
        }
    }
}
