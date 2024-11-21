//! Create API clients for remotes
//!
//! Make sure to call [`init`] to inject a concrete [`ClientFactory`]
//! instance before calling any of the provided functions.

use std::sync::OnceLock;

use anyhow::{bail, format_err, Error};
use http::uri::Authority;

use proxmox_client::{Client, TlsOptions};

use pdm_api_types::remotes::{Remote, RemoteType};
use pve_api_types::client::{PveClient, PveClientImpl};

use crate::pbs_client::PbsClient;

static INSTANCE: OnceLock<Box<dyn ClientFactory + Send + Sync>> = OnceLock::new();

/// Connection Info returned from [`prepare_connect_client`]
struct ConnectInfo {
    pub client: Client,
    pub prefix: String,
    pub perl_compat: bool,
}

/// Returns a [`proxmox_client::Client`] and a token prefix for the specified
/// [`pdm_api_types::Remote`]
fn prepare_connect_client(remote: &Remote) -> Result<ConnectInfo, Error> {
    let node = remote
        .nodes
        .first()
        .ok_or_else(|| format_err!("no nodes configured for remote"))?;
    let mut options = TlsOptions::default();

    if let Some(fp) = &node.fingerprint {
        options = TlsOptions::parse_fingerprint(fp)?;
    }

    let host_port: Authority = node.hostname.parse()?;

    let (default_port, prefix, perl_compat, pve_compat) = match remote.ty {
        RemoteType::Pve => (8006, "PVEAPIToken".to_string(), true, true),
        RemoteType::Pbs => (8007, "PBSAPIToken".to_string(), false, false),
    };

    let uri: http::uri::Uri = format!(
        "https://{}:{}",
        host_port.host(),
        host_port.port_u16().unwrap_or(default_port)
    )
    .parse()?;

    let mut client =
        proxmox_client::Client::with_options(uri.clone(), options, Default::default())?;
    client.set_pve_compatibility(pve_compat);

    Ok(ConnectInfo {
        client,
        prefix,
        perl_compat,
    })
}

/// Constructs a [`Client`] for the given [`Remote`] for an API token
///
/// It does not actually opens a connection there, but prepares the client with the correct
/// authentication information and settings for the [`RemoteType`]
fn connect(remote: &Remote) -> Result<Client, anyhow::Error> {
    let ConnectInfo {
        client,
        perl_compat,
        prefix,
    } = prepare_connect_client(remote)?;
    client.set_authentication(proxmox_client::Token {
        userid: remote.authid.to_string(),
        prefix,
        value: remote.token.to_string(),
        perl_compat,
    });

    Ok(client)
}

/// Constructs a [`Client`] for the given [`Remote`] for an API token or user
///
/// In case the remote has a user configured (instead of an API token), it will connect and get a
/// ticket, so that further connections are properly authenticated. Otherwise it behaves
/// identically as [`connect`].
///
/// This is intended for API calls that accept a user in addition to tokens.
///
/// Note: currently does not support two factor authentication.
async fn connect_or_login(remote: &Remote) -> Result<Client, anyhow::Error> {
    if remote.authid.is_token() {
        connect(remote)
    } else {
        let info = prepare_connect_client(remote)?;
        let client = info.client;
        match client
            .login(proxmox_login::Login::new(
                client.api_url().to_string(),
                remote.authid.to_string(),
                remote.token.to_string(),
            ))
            .await
        {
            Ok(Some(_)) => bail!("two factor auth not supported"),
            Ok(None) => {}
            Err(err) => match err {
                // FIXME: check why Api with 401 is returned instead of an Authentication error
                proxmox_client::Error::Api(code, _) if code.as_u16() == 401 => {
                    bail!("authentication failed")
                }
                proxmox_client::Error::Authentication(_) => {
                    bail!("authentication failed")
                }
                _ => return Err(err.into()),
            },
        }
        Ok(client)
    }
}

/// Abstract factory for creating remote clients.
#[async_trait::async_trait]
pub trait ClientFactory {
    /// Create a new API client for PVE remotes
    fn make_pve_client(&self, remote: &Remote) -> Result<Box<dyn PveClient + Send + Sync>, Error>;

    /// Create a new API client for PBS remotes
    fn make_pbs_client(&self, remote: &Remote) -> Result<Box<PbsClient>, Error>;

    /// Create a new API client for PVE remotes.
    ///
    /// In case the remote has a user configured (instead of an API token), it will connect and get
    /// a ticket, so that further connections are properly authenticated. Otherwise it behaves
    /// identically as [`make_pve_client`].
    ///
    /// This is intended for API calls that accept a user in addition to tokens.
    ///
    /// Note: currently does not support two factor authentication.
    async fn make_pve_client_and_login(
        &self,
        remote: &Remote,
    ) -> Result<Box<dyn PveClient + Send + Sync>, Error>;

    /// Create a new API client for PBS remotes.
    ///
    /// In case the remote has a user configured (instead of an API token), it will connect and get
    /// a ticket, so that further connections are properly authenticated. Otherwise it behaves
    /// identically as [`make_pbs_client`].
    ///
    /// This is intended for API calls that accept a user in addition to tokens.
    ///
    /// Note: currently does not support two factor authentication.
    async fn make_pbs_client_and_login(&self, remote: &Remote) -> Result<Box<PbsClient>, Error>;
}

/// Default production client factory
pub struct DefaultClientFactory;

#[async_trait::async_trait]
impl ClientFactory for DefaultClientFactory {
    fn make_pve_client(&self, remote: &Remote) -> Result<Box<dyn PveClient + Send + Sync>, Error> {
        let client = crate::connection::connect(remote)?;
        Ok(Box::new(PveClientImpl(client)))
    }

    fn make_pbs_client(&self, remote: &Remote) -> Result<Box<PbsClient>, Error> {
        let client = crate::connection::connect(remote)?;
        Ok(Box::new(PbsClient(client)))
    }

    async fn make_pve_client_and_login(
        &self,
        remote: &Remote,
    ) -> Result<Box<dyn PveClient + Send + Sync>, Error> {
        let client = connect_or_login(remote).await?;
        Ok(Box::new(PveClientImpl(client)))
    }

    async fn make_pbs_client_and_login(&self, remote: &Remote) -> Result<Box<PbsClient>, Error> {
        let client = connect_or_login(remote).await?;
        Ok(Box::new(PbsClient(client)))
    }
}

fn instance() -> &'static (dyn ClientFactory + Send + Sync) {
    // Not initializing the connection factory instance is
    // entirely in our reponsibility and not something we can recover from,
    // so it should be okay to panic in this case.
    INSTANCE
        .get()
        .expect("client factory instance not set")
        .as_ref()
}

/// Create a new API client for PVE remotes
pub fn make_pve_client(remote: &Remote) -> Result<Box<dyn PveClient + Send + Sync>, Error> {
    instance().make_pve_client(remote)
}

/// Create a new API client for PBS remotes
pub fn make_pbs_client(remote: &Remote) -> Result<Box<PbsClient>, Error> {
    instance().make_pbs_client(remote)
}

/// Create a new API client for PVE remotes.
///
/// In case the remote has a user configured (instead of an API token), it will connect and get a
/// ticket, so that further connections are properly authenticated. Otherwise it behaves
/// identically as [`make_pve_client`].
///
/// This is intended for API calls that accept a user in addition to tokens.
///
/// Note: currently does not support two factor authentication.
pub async fn make_pve_client_and_login(
    remote: &Remote,
) -> Result<Box<dyn PveClient + Send + Sync>, Error> {
    instance().make_pve_client_and_login(remote).await
}

/// Create a new API client for PBS remotes.
///
/// In case the remote has a user configured (instead of an API token), it will connect and get a
/// ticket, so that further connections are properly authenticated. Otherwise it behaves
/// identically as [`make_pbs_client`].
///
/// This is intended for API calls that accept a user in addition to tokens.
///
/// Note: currently does not support two factor authentication.
pub async fn make_pbs_client_and_login(remote: &Remote) -> Result<Box<PbsClient>, Error> {
    instance().make_pbs_client_and_login(remote).await
}

/// Initialize the [`ClientFactory`] instance.
///
/// Will panic if the instance has already been set.
pub fn init(instance: Box<dyn ClientFactory + Send + Sync>) {
    if INSTANCE.set(instance).is_err() {
        panic!("connection factory instance already set");
    }
}
