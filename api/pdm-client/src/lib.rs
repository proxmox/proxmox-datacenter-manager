//! Proxmox Datacenter Manager API client.

use anyhow::{format_err, Error};
use openssl::x509;
use serde::{Deserialize, Serialize};

use proxmox_client::Environment;

use pdm_api_types::{Authid, PveRemote};

/// In the future we may also have PMG or PBS nodes.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Remote {
    Pve(PveRemote),
}

pub struct Client<E: Environment> {
    client: proxmox_client::HyperClient<E>,
    auth_id: Authid,
}

impl<E> Client<E>
where
    E: Environment,
    E::Error: From<anyhow::Error>,
    anyhow::Error: From<E::Error>,
{
    pub fn new(env: E, server: &str, auth_id: Authid, options: Options) -> Result<Self, E::Error> {
        use proxmox_client::TlsOptions;

        let tls_options = match options.callback {
            Some(cb) => TlsOptions::Callback(cb),
            None => TlsOptions::default(),
        };

        let client = proxmox_client::HyperClient::with_options(
            server
                .parse()
                .map_err(|err| format_err!("bad address: {server:?} - {err}"))?,
            env,
            tls_options,
            options.http_options,
        )?;

        Ok(Self { client, auth_id })
    }

    pub async fn login(&self) -> Result<(), Error> {
        self.client.login().await?;
        Ok(())
    }

    pub async fn remote_list(&self) -> Result<Vec<Remote>, Error> {
        Ok(self.client.get("/api2/extjs/remotes").await?.data)
    }
}

#[derive(Default)]
// TODO: Merge this with pbs-client's stuff
pub struct Options {
    /// Set a TLS verification callback.
    callback:
        Option<Box<dyn Fn(bool, &mut x509::X509StoreContextRef) -> bool + Send + Sync + 'static>>,

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

    /// Set the HTTP related options.
    pub fn http_options(mut self, http_options: proxmox_http::HttpOptions) -> Self {
        self.http_options = http_options;
        self
    }
}
