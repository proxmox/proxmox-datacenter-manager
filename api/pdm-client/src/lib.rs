//! Proxmox Datacenter Manager API client.

use anyhow::{format_err, Error};
use openssl::x509;

use proxmox_client::Environment;

use pdm_api_types::Remote;

pub struct Client<E: Environment> {
    client: proxmox_client::HyperClient<E>,
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

        Ok(Self { client })
    }

    pub async fn login(&self) -> Result<(), Error> {
        self.client.login().await?;
        Ok(())
    }

    pub async fn list_remotes(&self) -> Result<Vec<Remote>, Error> {
        Ok(self
            .client
            .get("/api2/extjs/remotes")
            .await?
            .into_data_or_err()?)
    }

    pub async fn add_remote(&self, remote: &Remote) -> Result<(), Error> {
        self.client
            .post::<_, ()>("/api2/extjs/remotes", remote)
            .await?;
        Ok(())
    }

    pub async fn update_remote(
        &self,
        remote: &str,
        updater: &pdm_api_types::PveRemoteUpdater,
    ) -> Result<(), Error> {
        let path = format!("/api2/extjs/remotes/{remote}");
        self.client.put(&path, updater).await?.nodata()?;
        Ok(())
    }

    pub async fn remove_remote(&self, remote: &str) -> Result<(), Error> {
        let path = format!("/api2/extjs/remotes/{remote}");
        self.client.delete(&path).await?.nodata()?;
        Ok(())
    }

    pub async fn remote_version(
        &self,
        remote: &str,
    ) -> Result<pve_client::types::VersionResponse, Error> {
        let path = format!("/api2/extjs/remotes/{remote}/version");
        Ok(self.client.get(&path).await?.into_data_or_err()?)
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
