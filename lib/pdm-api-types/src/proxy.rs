use anyhow::{bail, Error};
use http::Uri;

use proxmox_schema::{ApiStringFormat, Schema, StringSchema};

fn verify_proxy_url(http_proxy: &str) -> Result<(), Error> {
    let proxy_uri: Uri = http_proxy.parse()?;
    if proxy_uri.authority().is_none() {
        bail!("missing proxy authority");
    };

    match proxy_uri.scheme_str() {
        Some("http") => { /* Ok */ }
        Some(scheme) => bail!("unsupported proxy scheme '{}'", scheme),
        None => { /* assume HTTP */ }
    }

    Ok(())
}

pub const HTTP_PROXY_SCHEMA: Schema =
    StringSchema::new("HTTP proxy configuration [http://]<host>[:port]")
        .format(&ApiStringFormat::VerifyFn(|s| {
            verify_proxy_url(s)?;
            Ok(())
        }))
        .min_length(1)
        .max_length(128)
        .type_text("[http://]<host>[:port]")
        .schema();
