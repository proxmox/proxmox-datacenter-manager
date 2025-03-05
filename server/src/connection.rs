//! Create API clients for remotes
//!
//! Make sure to call [`init`] to inject a concrete [`ClientFactory`]
//! instance before calling any of the provided functions.

use std::collections::HashMap;
use std::future::Future;
use std::pin::{pin, Pin};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::Once;
use std::sync::{LazyLock, OnceLock};
use std::time::{Duration, SystemTime};

use anyhow::{bail, format_err, Error};
use http::uri::Authority;
use http::Method;
use serde::Serialize;

use proxmox_client::{Client, HttpApiClient, HttpApiResponse, HttpApiResponseStream, TlsOptions};

use pdm_api_types::remotes::{NodeUrl, Remote, RemoteType};
use pve_api_types::client::PveClientImpl;

use crate::pbs_client::PbsClient;

static INSTANCE: OnceLock<Box<dyn ClientFactory + Send + Sync>> = OnceLock::new();

/// Connection Info returned from [`prepare_connect_client`]
struct ConnectInfo {
    prefix: String,
    perl_compat: bool,
    pve_compat: bool,
    default_port: u16,
}

impl ConnectInfo {
    fn for_remote(remote: &Remote) -> Self {
        let (default_port, prefix, perl_compat, pve_compat) = match remote.ty {
            RemoteType::Pve => (8006, "PVEAPIToken".to_string(), true, true),
            RemoteType::Pbs => (8007, "PBSAPIToken".to_string(), false, false),
        };

        ConnectInfo {
            prefix,
            perl_compat,
            pve_compat,
            default_port,
        }
    }
}
///
/// Returns a [`proxmox_client::Client`] set up to connect to a specific node.
fn prepare_connect_client_to_node(
    node: &NodeUrl,
    default_port: u16,
    pve_compat: bool,
) -> Result<Client, Error> {
    let mut options = TlsOptions::default();

    if let Some(fp) = &node.fingerprint {
        options = TlsOptions::parse_fingerprint(fp)?;
    }

    let host_port: Authority = node.hostname.parse()?;

    let uri: http::uri::Uri = format!(
        "https://{}:{}",
        host_port.host(),
        host_port.port_u16().unwrap_or(default_port)
    )
    .parse()?;

    let mut client =
        proxmox_client::Client::with_options(uri.clone(), options, Default::default())?;
    client.set_pve_compatibility(pve_compat);
    Ok(client)
}

/// Returns a [`proxmox_client::Client`] and connection info required to set token authentication
/// data for the [`pdm_api_types::Remote`].
fn prepare_connect_client(
    remote: &Remote,
    target_endpoint: Option<&str>,
) -> Result<(Client, ConnectInfo), Error> {
    let node = remote
        .nodes
        .iter()
        .find(|endpoint| match target_endpoint {
            Some(target) => target == endpoint.hostname,
            None => true,
        })
        .ok_or_else(|| match target_endpoint {
            Some(endpoint) => format_err!("{endpoint} not configured for remote"),
            None => format_err!("no nodes configured for remote"),
        })?;

    let info = ConnectInfo::for_remote(remote);

    let client = prepare_connect_client_to_node(node, info.default_port, info.pve_compat)?;

    Ok((client, info))
}

/// Constructs a [`Client`] for the given [`Remote`] for an API token
///
/// It does not actually opens a connection there, but prepares the client with the correct
/// authentication information and settings for the [`RemoteType`]
fn connect(remote: &Remote, target_endpoint: Option<&str>) -> Result<Client, anyhow::Error> {
    let (client, info) = prepare_connect_client(remote, target_endpoint)?;
    client.set_authentication(proxmox_client::Token {
        userid: remote.authid.to_string(),
        value: remote.token.to_string(),
        prefix: info.prefix,
        perl_compat: info.perl_compat,
    });
    Ok(client)
}

/// Returns a [`MultiClient`] and connection info required to set token authentication
/// data for the [`pdm_api_types::Remote`].
fn prepare_connect_multi_client(remote: &Remote) -> Result<(MultiClient, ConnectInfo), Error> {
    if remote.nodes.is_empty() {
        bail!("no nodes configured for remote");
    };

    let info = ConnectInfo::for_remote(remote);

    let mut clients = Vec::new();

    for node in &remote.nodes {
        clients.push(Arc::new(prepare_connect_client_to_node(
            node,
            info.default_port,
            info.pve_compat,
        )?));
    }

    Ok((MultiClient::new(clients), info))
}

/// Like [`connect()`], but with failover support for remotes which can have multiple nodes.
fn multi_connect(remote: &Remote) -> Result<MultiClient, anyhow::Error> {
    let (client, info) = prepare_connect_multi_client(remote)?;

    client.for_each_client(|client| {
        client.set_authentication(proxmox_client::Token {
            userid: remote.authid.to_string(),
            value: remote.token.to_string(),
            prefix: info.prefix.clone(),
            perl_compat: info.perl_compat,
        });
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
async fn connect_or_login(
    remote: &Remote,
    target_endpoint: Option<&str>,
) -> Result<Client, anyhow::Error> {
    if remote.authid.is_token() {
        connect(remote, target_endpoint)
    } else {
        let (client, _info) = prepare_connect_client(remote, target_endpoint)?;
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
    fn make_pve_client(&self, remote: &Remote) -> Result<Arc<PveClient>, Error>;

    /// Create a new API client for PBS remotes
    fn make_pbs_client(&self, remote: &Remote) -> Result<Box<PbsClient>, Error>;

    /// Create a new API client for PVE remotes, but with a specific endpoint.
    fn make_pve_client_with_endpoint(
        &self,
        remote: &Remote,
        target_endpoint: Option<&str>,
    ) -> Result<Arc<PveClient>, Error>;

    /// Create a new API client for PVE remotes.
    ///
    /// In case the remote has a user configured (instead of an API token), it will connect and get
    /// a ticket, so that further connections are properly authenticated. Otherwise it behaves
    /// identically as [`make_pve_client`].
    ///
    /// This is intended for API calls that accept a user in addition to tokens.
    ///
    /// Note: currently does not support two factor authentication.
    async fn make_pve_client_and_login(&self, remote: &Remote) -> Result<Arc<PveClient>, Error>;

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

pub type PveClient = dyn pve_api_types::client::PveClient + Send + Sync;

/// A cached client for a remote (to reuse connections and share info about connection issues in
/// remotes with multiple nodes...).
struct ClientEntry<T: ?Sized> {
    last_used: SystemTime,
    client: Arc<T>,
    remote: Remote,
}

/// Contains the cached clients and handle to the future dealing with timing them out.
#[derive(Default)]
struct ConnectionCache {
    pve_clients: StdMutex<HashMap<String, ClientEntry<PveClient>>>,
}

/// This cache is a singleton.
static CONNECTION_CACHE: LazyLock<ConnectionCache> = LazyLock::new(Default::default);
static CLEANUP_FUTURE_STARTED: Once = Once::new();

impl ConnectionCache {
    const CLEANUP_INTERVAL: Duration = Duration::from_secs(30);
    const STALE_TIMEOUT: Duration = Duration::from_secs(30);

    /// Access the cache
    fn get() -> &'static Self {
        let this = &CONNECTION_CACHE;
        this.init();
        this
    }

    /// If it hasn't already, spawn the cleanup future.
    fn init(&self) {
        CLEANUP_FUTURE_STARTED.call_once(|| {
            tokio::spawn(async move {
                let future = pin!(CONNECTION_CACHE.cleanup_future());
                let abort_future = pin!(proxmox_daemon::shutdown_future());
                futures::future::select(future, abort_future).await;
            });
        });
    }

    /// Run a cleanup operation every 30 seconds.
    async fn cleanup_future(&self) {
        loop {
            tokio::time::sleep(Self::CLEANUP_INTERVAL).await;
            self.cleanup_cycle();
        }
    }

    /// Clean out cached clients older than 30 seconds.
    fn cleanup_cycle(&self) {
        let oldest_time = SystemTime::now() - Self::STALE_TIMEOUT;
        self.pve_clients
            .lock()
            .unwrap()
            .retain(|_remote_name, client| client.last_used >= oldest_time)
    }

    fn make_pve_client(&self, remote: &Remote) -> Result<Arc<PveClient>, anyhow::Error> {
        let mut pve_clients = self.pve_clients.lock().unwrap();
        if let Some(client) = pve_clients.get_mut(&remote.id) {
            // Verify the remote is still the same:
            if client.remote == *remote {
                client.last_used = SystemTime::now();
                return Ok(Arc::clone(&client.client));
            }
        }

        let client: Arc<PveClient> =
            Arc::new(PveClientImpl(crate::connection::multi_connect(remote)?));
        pve_clients.insert(
            remote.id.clone(),
            ClientEntry {
                last_used: SystemTime::now(),
                client: Arc::clone(&client),
                remote: remote.clone(),
            },
        );
        Ok(client)
    }
}

#[async_trait::async_trait]
impl ClientFactory for DefaultClientFactory {
    fn make_pve_client(&self, remote: &Remote) -> Result<Arc<PveClient>, Error> {
        ConnectionCache::get().make_pve_client(remote)
    }

    fn make_pbs_client(&self, remote: &Remote) -> Result<Box<PbsClient>, Error> {
        let client = crate::connection::connect(remote, None)?;
        Ok(Box::new(PbsClient(client)))
    }

    fn make_pve_client_with_endpoint(
        &self,
        remote: &Remote,
        target_endpoint: Option<&str>,
    ) -> Result<Arc<PveClient>, Error> {
        let client = crate::connection::connect(remote, target_endpoint)?;
        Ok(Arc::new(PveClientImpl(client)))
    }

    async fn make_pve_client_and_login(&self, remote: &Remote) -> Result<Arc<PveClient>, Error> {
        let client = connect_or_login(remote, None).await?;
        Ok(Arc::new(PveClientImpl(client)))
    }

    async fn make_pbs_client_and_login(&self, remote: &Remote) -> Result<Box<PbsClient>, Error> {
        let client = connect_or_login(remote, None).await?;
        Ok(Box::new(PbsClient(client)))
    }
}

fn instance() -> &'static (dyn ClientFactory + Send + Sync) {
    // Not initializing the connection factory instance is
    // entirely in our responsibility and not something we can recover from,
    // so it should be okay to panic in this case.
    INSTANCE
        .get()
        .expect("client factory instance not set")
        .as_ref()
}

/// Create a new API client for PVE remotes
pub fn make_pve_client(remote: &Remote) -> Result<Arc<PveClient>, Error> {
    instance().make_pve_client(remote)
}

/// Create a new API client for PVE remotes, but for a specific endpoint
pub fn make_pve_client_with_endpoint(
    remote: &Remote,
    target_endpoint: Option<&str>,
) -> Result<Arc<PveClient>, Error> {
    instance().make_pve_client_with_endpoint(remote, target_endpoint)
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
pub async fn make_pve_client_and_login(remote: &Remote) -> Result<Arc<PveClient>, Error> {
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

/// This is another wrapper around the actual HTTP client responsible for dealing with connection
/// problems: if we cannot reach a node of a cluster, this will attempt to retry a request on
/// another node.
///
/// # Possible improvements
///
/// - For `GET` requests we could also start a 2nd request after a shorter time out (eg. 10s).
/// - We could use RRD data for a "best guess" where to start eg. if we know a node was offline on
///   the last rrd polling we'd start with a different one.
///   For this, we still need to include the node names in the clients here somehow so that we can
///   actually manage/re-shuffle them from the outside after this struct is already created.
struct MultiClient {
    state: StdMutex<MultiClientState>,
    timeout: Duration,
}

impl MultiClient {
    fn new(clients: Vec<Arc<Client>>) -> Self {
        Self {
            state: StdMutex::new(MultiClientState::new(clients)),
            timeout: Duration::from_secs(60),
        }
    }

    fn for_each_client<F>(&self, func: F)
    where
        F: Fn(&Arc<Client>),
    {
        for client in &self.state.lock().unwrap().clients {
            func(client);
        }
    }
}

/// Keeps track of which client (iow. which specific node of a remote) we're supposed to be using
/// right now.
struct MultiClientState {
    /// The current index *not* modulo the client count.
    current: usize,
    clients: Vec<Arc<Client>>,
}

impl MultiClientState {
    fn new(clients: Vec<Arc<Client>>) -> Self {
        Self {
            current: 0,
            clients,
        }
    }

    /// Whenever a request fails with the *current* client we move the current entry forward.
    ///
    /// # Note:
    ///
    /// With our current strategy `failed_index` is always less than `current`, but if we change
    /// the strategy, we may want to change this to something like `1 + max(current, failed)`.
    fn failed(&mut self, failed_index: usize) {
        if self.current == failed_index {
            self.current = self.current.wrapping_add(1);
        }
    }

    /// Get `current` as an *index* (i.e. modulo `clients.len()`).
    fn index(&self) -> usize {
        self.current % self.clients.len()
    }

    /// Get the current client and its index which can be passed to `failed()` if the client fails
    /// to connect.
    fn get(&self) -> (Arc<Client>, usize) {
        let index = self.index();
        (Arc::clone(&self.clients[index]), self.current)
    }

    /// Check if we already tried all clients since a specific starting index.
    ///
    /// When an API request is made we loop through the possible clients.
    /// Since multiple requests might be running simultaneously, it's possible that multiple tasks
    /// mark the same *or* *multiple* clients as failed already.
    ///
    /// We don't want to try clients which we know are currently non-functional, so a
    /// request-retry-loop will fail as soon as the same *number* of clients since its starting
    /// point were marked as faulty without retrying them all.
    fn tried_all_since(&self, start: usize) -> bool {
        self.tried_clients(start) >= self.clients.len()
    }

    /// We store the current index continuously without wrapping it modulo the client count (and
    /// only do that when indexing the `clients` array), so that we can easily check if "all
    /// currently running tasks taken together" have already tried all clients by comparing our
    /// starting point to the current index.
    fn tried_clients(&self, start: usize) -> usize {
        self.current.wrapping_sub(start)
    }
}

impl MultiClient {
    /// This is the client usage strategy.
    ///
    /// This is basically a "generator" for clients to try.
    ///
    /// We share the "state" with other tasks. When a client fails, it is "marked" as failed and
    /// the state "rotates" through the clients.
    /// We might be skipping clients if other tasks already tried "more" clients, but that's fine,
    /// since there's no point in trying the same remote twice simultaneously if it is currently
    /// offline...
    fn try_clients(&self) -> impl Iterator<Item = Arc<Client>> + '_ {
        let mut start_current = None;
        let state = &self.state;
        std::iter::from_fn(move || {
            let mut state = state.lock().unwrap();
            match start_current {
                None => {
                    // first attempt, just use the current client and remember the starting index
                    let (client, index) = state.get();
                    start_current = Some((index, index));
                    Some(client)
                }
                Some((start, current)) => {
                    // If our last request failed, the retry-loop asks for another client, mark the
                    // one we just used as failed and check if all clients have gone through a
                    // retry loop...
                    state.failed(current);
                    if state.tried_all_since(start) {
                        // This iterator (and therefore this retry-loop) has tried all clients.
                        // Give up.
                        return None;
                    }
                    // finally just get the new current client and update `current` for the later
                    // call to `failed()`
                    let (client, current) = state.get();
                    start_current = Some((start, current));
                    Some(client)
                }
            }
        })
        .fuse()
    }
}

// doing this via a generic method is currently tedious as it requires an extra helper trait to
// declare the flow of the lifetime in the `self.request` vs `self.streaming_request` function from
// its input to its generic output future... and then you run into borrow-checker limitations...
macro_rules! try_request {
    ($self:expr, $method:expr, $path_and_query:expr, $params:expr, $how:ident) => {
        let params = $params.map(serde_json::to_value);
        Box::pin(async move {
            let params = params
                .transpose()
                .map_err(|err| proxmox_client::Error::Anyhow(err.into()))?;

            let mut last_err = None;
            let mut timed_out = false;
            // The iterator in use here will automatically mark a client as faulty if we move on to
            // the `next()` one.
            for client in $self.try_clients() {
                if let Some(err) = last_err.take() {
                    log::error!("API client error, trying another remote - {err:?}");
                }
                if timed_out {
                    timed_out = false;
                    log::error!("API client timed out, trying another remote");
                }

                let request = client.$how($method.clone(), $path_and_query, params.as_ref());
                match tokio::time::timeout($self.timeout, request).await {
                    Ok(Err(proxmox_client::Error::Client(err))) => {
                        last_err = Some(err);
                    }
                    Ok(result) => return result,
                    Err(_) => {
                        timed_out = true;
                    }
                }
            }

            if let Some(err) = last_err {
                log::error!("API client error (giving up) - {err:?}");
            } else if timed_out {
                log::error!("API client timed out, no remotes reachable, giving up");
            }

            Err(proxmox_client::Error::Other(
                "failed to perform API request",
            ))
        })
    };
}

impl HttpApiClient for MultiClient {
    type ResponseFuture<'a> =
        Pin<Box<dyn Future<Output = Result<HttpApiResponse, proxmox_client::Error>> + Send + 'a>>;

    type ResponseStreamFuture<'a> = Pin<
        Box<
            dyn Future<Output = Result<HttpApiResponseStream<Self::Body>, proxmox_client::Error>>
                + Send
                + 'a,
        >,
    >;
    type Body = hyper::Body;

    fn request<'a, T>(
        &'a self,
        method: Method,
        path_and_query: &'a str,
        params: Option<T>,
    ) -> Self::ResponseFuture<'a>
    where
        T: Serialize + 'a,
    {
        try_request! { self, method, path_and_query, params, request }
    }

    fn streaming_request<'a, T>(
        &'a self,
        method: Method,
        path_and_query: &'a str,
        params: Option<T>,
    ) -> Self::ResponseStreamFuture<'a>
    where
        T: Serialize + 'a,
    {
        try_request! { self, method, path_and_query, params, streaming_request }
    }
}
