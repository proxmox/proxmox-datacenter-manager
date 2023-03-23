//! Proxmox VE API client.

use anyhow::{format_err, Error};
use openssl::x509;

use proxmox_client::Environment;

pub mod helpers;

#[path = "generated/code.rs"]
mod code;
#[path = "generated/types.rs"]
pub mod types;

pub struct Client<E: Environment> {
    pub client: proxmox_client::HyperClient<E>,
}

impl<E> Client<E>
where
    E: Environment,
    E::Error: From<anyhow::Error>,
    anyhow::Error: From<E::Error>,
{
    pub fn new(env: E, server: &str, options: Options) -> Result<Self, E::Error> {
        use proxmox_client::TlsOptions;

        let tls_options = match options.callback {
            Some(cb) => TlsOptions::Callback(cb),
            None => match options.fingerprint {
                Some(fp) => TlsOptions::Fingerprint(fp.to_vec()),
                None => TlsOptions::default(),
            },
        };

        let client = proxmox_client::HyperClient::with_options(
            format!("https://{server}:8006")
                .parse()
                .map_err(|err| format_err!("bad address: {server:?} - {err}"))?,
            env,
            tls_options,
            options.http_options,
        )?;

        Ok(Self { client })
    }

    pub async fn login(&self) -> Result<(), Error> {
        self.client.login().await?;
        Ok(())
    }
}

#[derive(Default)]
// TODO: Merge this with pbs-client's stuff
pub struct Options {
    /// Set a TLS verification callback.
    callback:
        Option<Box<dyn Fn(bool, &mut x509::X509StoreContextRef) -> bool + Send + Sync + 'static>>,

    fingerprint: Option<Vec<u8>>,

    /// `proxmox_http` based options.
    http_options: proxmox_http::HttpOptions,
}

impl Options {
    /// New default instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a TLS verification callback.
    pub fn tls_callback<F>(mut self, cb: F) -> Self
    where
        F: Fn(bool, &mut x509::X509StoreContextRef) -> bool + Send + Sync + 'static,
    {
        self.callback = Some(Box::new(cb));
        self
    }

    /// Expect a specific tls fingerprint. Does not take effect if `tls_callback` is used.
    pub fn tls_fingerprint_str(mut self, fingerprint: &str) -> Result<Self, Error> {
        self.fingerprint = Some(parse_fingerprint(fingerprint)?.to_vec());
        Ok(self)
    }

    /// Set the HTTP related options.
    pub fn http_options(mut self, http_options: proxmox_http::HttpOptions) -> Self {
        self.http_options = http_options;
        self
    }
}

fn parse_fingerprint(s: &str) -> Result<[u8; 32], Error> {
    use hex::FromHex;

    let hex: Vec<u8> = s
        .as_bytes()
        .iter()
        .copied()
        .filter(|&b| b != b':')
        .collect();

    <[u8; 32]>::from_hex(&hex).map_err(|_| format_err!("failed to parse fingerprint"))
}
