use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use http::request::Request;
use http::response::Response;
use http::uri::PathAndQuery;
use http::{StatusCode, Uri};
use serde_json::Value;

use proxmox_login::{Login, TicketResult};

use crate::auth::AuthenticationKind;
use crate::{Authentication, Environment, Error, Token};

/// HTTP client backend trait.
///
/// An async [`Client`] requires some kind of async HTTP client implementation.
pub trait HttpClient: Send + Sync {
    type Error: Error;
    type Request: Future<Output = Result<Response<Vec<u8>>, Self::Error>> + Send;

    fn request(&self, request: Request<Vec<u8>>) -> Self::Request;
}

/// In a cluster we may be able to connect to a different node if one connection fails.
struct ApiUrls {
    /// This is the list of cluster node URls.
    urls: Vec<Uri>,

    /// This is the current "good" URL. If we fail to connect here, we'll walk around the `urls`
    /// vec once before failing completely.
    /// Once a "good" URL is reached, we update this.
    current: AtomicUsize,

    /// Since another thread might be doing the same thing simultaneously, let's use this to keep
    /// track of when some thread has updated `current`. If we see a `generation` bump while
    /// probing URLs, we'll retry the new `current`.
    generation: AtomicU32,
}

impl ApiUrls {
    fn new(uri: Uri) -> Self {
        Self {
            urls: vec![uri],
            current: AtomicUsize::new(0),
            generation: AtomicU32::new(0),
        }
    }

    fn index(&self) -> usize {
        self.current.load(Ordering::Relaxed)
    }

    fn generation(&self) -> u32 {
        self.generation.load(Ordering::Acquire)
    }
}

/// Proxmox VE high level API client.
pub struct Client<C, E: Environment> {
    env: E,
    api_urls: ApiUrls,
    auth: StdMutex<Option<Arc<AuthenticationKind>>>,
    client: C,
    pve_compat: bool,
}

impl<C, E> Client<C, E>
where
    E: Environment,
{
    /// Get a reference to the current authentication information.
    pub fn authentication(&self) -> Option<Arc<AuthenticationKind>> {
        self.auth.lock().unwrap().clone()
    }

    pub fn use_api_token(&self, token: Token) {
        *self.auth.lock().unwrap() = Some(Arc::new(token.into()));
    }
}

fn to_request<E: Error>(request: proxmox_login::Request) -> Result<http::Request<Vec<u8>>, E> {
    http::Request::builder()
        .method(request.method)
        .uri(request.url)
        .header(http::header::CONTENT_TYPE, request.content_type)
        .header(
            http::header::CONTENT_LENGTH,
            request.content_length.to_string(),
        )
        .body(request.body)
        .map_err(E::internal)
}

impl<C, E: Environment> Client<C, E> {
    /// Enable Proxmox VE login API compatibility. This is required to support TFA authentication
    /// on Proxmox VE APIs which require the `new-format` option.
    pub fn set_pve_compatibility(&mut self, compatibility: bool) {
        self.pve_compat = compatibility;
    }
}

impl<C, E> Client<C, E>
where
    E: Environment,
    C: HttpClient,
    E::Error: From<C::Error>,
{
    /// Instantiate a client for an API with a given environment and HTTP client instance.
    pub fn with_client(api_url: Uri, environment: E, client: C) -> Self {
        Self {
            env: environment,
            api_urls: ApiUrls::new(api_url),
            auth: StdMutex::new(None),
            client,
            pve_compat: false,
        }
    }

    pub async fn login_auth(&self) -> Result<Arc<AuthenticationKind>, E::Error> {
        self.login().await?;
        self.auth
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| E::Error::internal("login failed to set authentication information"))
    }

    /// If currently logged in, this will fill in the auth cookie and CSRFPreventionToken header
    /// and return `Ok(request)`, otherwise it'll return `Err(request)` with the request
    /// unmodified.
    pub fn try_set_auth_headers(
        &self,
        request: http::request::Builder,
    ) -> Result<http::request::Builder, http::request::Builder> {
        let auth = self.auth.lock().unwrap().clone();
        match auth {
            Some(auth) => Ok(auth.set_auth_headers(request)),
            None => Err(request),
        }
    }

    /// Convenience method to login and set the authentication headers for a request.
    pub async fn set_auth_headers(
        &self,
        request: http::request::Builder,
    ) -> Result<http::request::Builder, E::Error> {
        Ok(self.login_auth().await?.set_auth_headers(request))
    }

    /// Ensure that we have a valid ticket.
    ///
    /// This will first attempt to load a ticket from the provided [`Environment`]. If successful,
    /// its expiration time will be verified.
    ///
    /// If no valid ticket is available already, this will connect to the PVE API and perform
    /// authentication.
    pub async fn login(&self) -> Result<(), E::Error> {
        let mut url_index = self.api_urls.index();
        let current_url = &self.api_urls.urls[url_index];

        let (userid, login) = self.need_login(current_url).await?;
        let login = match login {
            None => return Ok(()),
            Some(login) => login,
        };

        let mut login = login.pve_compatibility(self.pve_compat);

        let mut retry = None;
        let generation = self.api_urls.generation();

        // remember the finally successful address
        let retry_success = |retry: &mut Option<usize>, url_index: usize| {
            if retry.is_some() {
                *retry = None;
                if self.api_urls.generation() == generation {
                    self.api_urls.current.store(url_index, Ordering::Relaxed);
                    self.api_urls
                        .generation
                        .store(generation + 1, Ordering::Release);
                }
            }
        };

        // check whether we should be retrying a new address
        let should_retry =
            |retry: &mut Option<usize>, login: &mut Login, url_index: &mut usize| -> bool {
                match *retry {
                    Some(retry) => {
                        if retry == *url_index {
                            return false;
                        }
                    }
                    None => *retry = Some(*url_index),
                }

                // if another thread successfully found a working URL already, use that as our last
                // attempt:
                if self.api_urls.generation() != generation {
                    *url_index = self.api_urls.index();
                    *retry = Some(*url_index);
                    return true;
                }

                // otherwise cycle through the available addresses:
                *url_index = (*url_index + 1) % self.api_urls.urls.len();
                login.set_url(self.api_urls.urls[*url_index].to_string());

                true
            };

        loop {
            let current_url = &self.api_urls.urls[url_index];
            let response = match self
                .client
                .request(to_request(login.request().map_err(E::Error::internal)?)?)
                .await
            {
                Ok(r) => {
                    retry_success(&mut retry, url_index);
                    r
                }
                Err(err) => {
                    if should_retry(&mut retry, &mut login, &mut url_index) {
                        continue;
                    }
                    return Err(err.into());
                }
            };

            if !response.status().is_success() {
                // FIXME: does `http` somehow expose the status string?
                return Err(E::Error::api_error(
                    response.status(),
                    "authentication failed",
                ));
            }

            let challenge = match login.response(response.body()).map_err(E::Error::bad_api)? {
                TicketResult::Full(auth) => {
                    return self.finish_auth(current_url, &userid, auth).await
                }
                TicketResult::TfaRequired(challenge) => challenge,
            };

            let response = self
                .env
                .query_second_factor_async(current_url, &userid, &challenge.challenge)
                .await?;

            let response = match self
                .client
                .request(to_request(
                    challenge
                        .respond_raw(&response)
                        .map_err(E::Error::internal)?,
                )?)
                .await
            {
                Ok(r) => {
                    retry_success(&mut retry, url_index);
                    r
                }
                Err(err) => {
                    if should_retry(&mut retry, &mut login, &mut url_index) {
                        continue;
                    }
                    return Err(err.into());
                }
            };

            let status = response.status();
            if !status.is_success() {
                return Err(E::Error::api_error(status, "authentication failed"));
            }

            let auth = challenge
                .response(response.body())
                .map_err(E::Error::bad_api)?;

            break self.finish_auth(current_url, &userid, auth).await;
        }
    }

    /// Get the current username and, if required, a `Login` request.
    async fn need_login(&self, current_url: &Uri) -> Result<(String, Option<Login>), E::Error> {
        use proxmox_login::ticket::Validity;

        let (userid, auth) = self.current_auth().await?;

        let authkind = match auth {
            None => {
                let password = self.env.query_password_async(current_url, &userid).await?;
                return Ok((
                    userid.clone(),
                    Some(Login::new(current_url.to_string(), userid, password)),
                ));
            }
            Some(authkind) => authkind,
        };

        let auth = match &*authkind {
            AuthenticationKind::Token(_) => return Ok((userid, None)),
            AuthenticationKind::Ticket(auth) => auth,
        };

        Ok(match auth.ticket.validity() {
            Validity::Valid => {
                *self.auth.lock().unwrap() = Some(authkind);
                (userid, None)
            }
            Validity::Refresh => (
                userid,
                Some(
                    Login::renew(current_url.to_string(), auth.ticket.to_string())
                        .map_err(E::Error::custom)?,
                ),
            ),

            Validity::Expired => {
                let password = self.env.query_password_async(current_url, &userid).await?;
                (
                    userid.clone(),
                    Some(Login::new(current_url.to_string(), userid, password)),
                )
            }
        })
    }

    /// Store the authentication info in our `auth` field and notify the environment.
    async fn finish_auth(
        &self,
        current_url: &Uri,
        userid: &str,
        auth: Authentication,
    ) -> Result<(), E::Error> {
        let auth_string = serde_json::to_string(&auth).map_err(E::Error::internal)?;
        *self.auth.lock().unwrap() = Some(Arc::new(auth.into()));
        self.env
            .store_ticket_async(current_url, userid, auth_string.as_bytes())
            .await
    }

    /// Get the currently used API url from our array of possible cluster nodes.
    fn api_url(&self) -> &Uri {
        &self.api_urls.urls[self.api_urls.index()]
    }

    /// Get the current user id and a reference to the current authentication method.
    /// If not authenticated yet, authenticate.
    ///
    /// This may cause the environment to be queried for user ids/passwords/FIDO/...
    async fn current_auth(&self) -> Result<(String, Option<Arc<AuthenticationKind>>), E::Error> {
        let auth = self.auth.lock().unwrap().clone();

        let userid;
        let auth = match auth {
            Some(auth) => {
                userid = auth.userid().to_owned();
                Some(auth)
            }
            None => {
                userid = self.env.query_userid_async(self.api_url()).await?;
                self.reload_existing_ticket(&userid).await?
            }
        };

        Ok((userid, auth))
    }

    /// Attempt to load an existing ticket from the environment.
    async fn reload_existing_ticket(
        &self,
        userid: &str,
    ) -> Result<Option<Arc<AuthenticationKind>>, E::Error> {
        let ticket = match self.env.load_ticket_async(self.api_url(), userid).await? {
            Some(auth) => auth,
            None => return Ok(None),
        };

        let auth: Authentication = serde_json::from_slice(&ticket)
            .map_err(|err| E::Error::env(format!("bad ticket data: {err}")))?;

        let auth = Arc::new(auth.into());
        *self.auth.lock().unwrap() = Some(Arc::clone(&auth));
        Ok(Some(auth))
    }

    /// Build a URI relative to the current API endpoint.
    fn build_uri(&self, base_uri: Uri, path: &str) -> Result<Uri, E::Error> {
        let parts = base_uri.into_parts();
        let mut builder = http::uri::Builder::new();
        if let Some(scheme) = parts.scheme {
            builder = builder.scheme(scheme);
        }
        if let Some(authority) = parts.authority {
            builder = builder.authority(authority)
        }
        builder
            .path_and_query(path.parse::<PathAndQuery>().map_err(E::Error::internal)?)
            .build()
            .map_err(E::Error::internal)
    }

    /// Attempt to execute a request, while automatically trying to reach different cluster nodes
    /// if we fail to connect to the current node.
    ///
    /// The `make_request` closure gets the base `Uri` and should use it to build a `Request`.
    /// The `Request` is then attempted. If there's a connection issue, `make_request` will be
    /// called again with another cluster node (if available).
    /// Only if no node responds - or a legitimate HTTP error is produced - will the error be
    /// returned.
    async fn request_retry_loop<Fut>(
        &self,
        make_request: impl Fn(Uri) -> Fut,
    ) -> Result<Response<Vec<u8>>, E::Error>
    where
        Fut: Future<Output = Result<Request<Vec<u8>>, E::Error>> + Send,
    {
        let generation = self.api_urls.generation();
        let mut url_index = self.api_urls.index();
        let mut retry = None;
        loop {
            let err = match self
                .client
                .request(make_request(self.api_urls.urls[url_index].clone()).await?)
                .await
            {
                Ok(response) => {
                    if retry.is_some() && self.api_urls.generation() == generation {
                        self.api_urls.current.store(url_index, Ordering::Relaxed);
                        self.api_urls
                            .generation
                            .store(generation + 1, Ordering::Release);
                    }
                    return Ok(response);
                }
                Err(err) => err,
            };

            match retry {
                Some(retry) => {
                    if retry == url_index {
                        return Err(err.into());
                    }
                }
                None => retry = Some(url_index),
            }

            // if another thread successfully found a working URL already, use that as our last
            // attempt:
            if self.api_urls.generation() != generation {
                url_index = self.api_urls.index();
                retry = Some(url_index);
                continue;
            }

            url_index = (url_index + 1) % self.api_urls.urls.len();
        }
    }

    /// Execute a `GET` request, possibly trying multiple cluster nodes.
    pub(crate) async fn get_request<'a, R>(&'a self, uri: &str) -> Result<ApiResponse<R>, E::Error>
    where
        R: serde::de::DeserializeOwned,
    {
        self.login().await?;

        let response = self
            .request_retry_loop(|base_uri| async {
                self.set_auth_headers(Request::get(self.build_uri(base_uri, uri)?))
                    .await?
                    .body(Vec::new())
                    .map_err(Error::internal)
            })
            .await?;

        Self::handle_response(response)
    }

    /// Execute a `PUT` request with the given body, possibly trying multiple cluster nodes.
    pub(crate) async fn put_request<'a, B, R>(
        &'a self,
        uri: &str,
        body: &'a B,
    ) -> Result<ApiResponse<R>, E::Error>
    where
        B: serde::Serialize,
        R: serde::de::DeserializeOwned,
    {
        let auth = self.login_auth().await?;
        self.json_request(&auth, http::Method::PUT, uri, body).await
    }

    /// Execute a `POST` request with the given body, possibly trying multiple cluster nodes.
    pub(crate) async fn post_request<'a, B, R>(
        &'a self,
        uri: &str,
        body: &'a B,
    ) -> Result<ApiResponse<R>, E::Error>
    where
        B: serde::Serialize,
        R: serde::de::DeserializeOwned,
    {
        let auth = self.login_auth().await?;
        self.json_request(&auth, http::Method::POST, uri, body)
            .await
    }

    /// Helper method for a JSON request with a JSON body `B`, yielding a JSON result type `R`.
    pub(crate) async fn json_request<'a, B, R>(
        &'a self,
        auth: &'a AuthenticationKind,
        method: http::Method,
        uri: &str,
        body: &'a B,
    ) -> Result<ApiResponse<R>, E::Error>
    where
        B: serde::Serialize,
        R: serde::de::DeserializeOwned,
    {
        let body = serde_json::to_vec(&body).map_err(E::Error::internal)?;
        let content_length = body.len();
        self.json_request_bytes(auth, method, uri, body, content_length)
            .await
    }

    /// Helper method for a request with a byte body, yieldinig a JSON result of type `R`.
    async fn json_request_bytes<'a, R>(
        &'a self,
        auth: &AuthenticationKind,
        method: http::Method,
        uri: &str,
        body: Vec<u8>,
        content_length: usize,
    ) -> Result<ApiResponse<R>, E::Error>
    where
        R: serde::de::DeserializeOwned,
    {
        let response = self
            .run_json_request_with_body(auth, method, uri, body, content_length)
            .await?;
        Self::handle_response(response)
    }

    async fn run_json_request_with_body<'a>(
        &'a self,
        auth: &'a AuthenticationKind,
        method: http::Method,
        uri: &str,
        body: Vec<u8>,
        content_length: usize,
    ) -> Result<Response<Vec<u8>>, E::Error> {
        self.request_retry_loop(|base_uri| async {
            let request = Request::builder()
                .method(method.clone())
                .uri(self.build_uri(base_uri, uri)?)
                .header(http::header::CONTENT_TYPE, "application/json")
                .header(http::header::CONTENT_LENGTH, content_length.to_string());

            auth.set_auth_headers(request)
                .body(body.clone())
                .map_err(Error::internal)
        })
        .await
    }

    /// Check the status code, deserialize the json/extjs `RawApiResponse` and check for error
    /// messages inside.
    /// On success, deserialize the expected result type.
    fn handle_response<R>(response: Response<Vec<u8>>) -> Result<ApiResponse<R>, E::Error>
    where
        R: serde::de::DeserializeOwned,
    {
        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(E::Error::unauthorized());
        }

        if !response.status().is_success() {
            // FIXME: Decode json errors...
            //match serde_json::from_slice(&body)
            //    Ok(value) =>
            //        if value["error"]
            let (response, body) = response.into_parts();
            let body = String::from_utf8(body).map_err(Error::bad_api)?;
            return Err(E::Error::api_error(response.status, body));
        }

        let data: RawApiResponse<R> =
            serde_json::from_slice(&response.into_body()).map_err(Error::bad_api)?;

        data.check()
    }
}

pub struct ApiResponse<T> {
    pub data: T,
    pub attribs: HashMap<String, Value>,
}

#[derive(serde::Deserialize)]
struct RawApiResponse<T> {
    #[serde(default, deserialize_with = "proxmox_login::parse::deserialize_u16")]
    pub status: Option<u16>,
    pub message: Option<String>,
    #[serde(default, deserialize_with = "proxmox_login::parse::deserialize_u8")]
    pub success: Option<u8>,
    pub data: Option<T>,

    #[serde(default, flatten)]
    pub attribs: HashMap<String, Value>,
}

impl<T> RawApiResponse<T> {
    pub fn check<E: Error>(mut self) -> Result<ApiResponse<T>, E> {
        if self.success.unwrap_or(0) == 0 {
            let status = http::StatusCode::from_u16(self.status.unwrap_or(400))
                .unwrap_or(http::StatusCode::BAD_REQUEST);
            let message = self
                .message
                .take()
                .unwrap_or_else(|| "no message provided".to_string());
            return Err(E::api_error(status, message));
        }

        let data = self
            .data
            .take()
            .ok_or_else(|| E::bad_api("api returned no data"))?;

        Ok(ApiResponse {
            data,
            attribs: self.attribs,
        })
    }
}

#[cfg(feature = "hyper-client")]
pub type HyperClient<E> = Client<Arc<proxmox_http::client::Client>, E>;

#[cfg(feature = "hyper-client")]
impl<C, E> Client<C, E>
where
    E: Environment,
    E::Error: From<anyhow::Error>,
{
    /// Create a new client instance which will connect to the provided endpoint.
    pub fn new(api_url: Uri, environment: E) -> HyperClient<E> {
        Client::with_client(
            api_url,
            environment,
            Arc::new(proxmox_http::client::Client::new()),
        )
    }
}

#[cfg(feature = "hyper-client")]
mod hyper_client_extras {
    use std::future::Future;
    use std::sync::Arc;

    use anyhow::format_err;
    use http::request::Request;
    use http::response::Response;
    use http::Uri;
    use openssl::hash::MessageDigest;
    use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
    use openssl::x509::{self, X509};

    use proxmox_http::client::Client as ProxmoxClient;

    use super::{Client, HyperClient};
    use crate::Environment;

    #[derive(Clone, Debug, Default)]
    pub enum TlsOptions {
        /// Default TLS verification.
        #[default]
        Verify,

        /// Insecure: ignore invalid certificates.
        Insecure,

        /// Expect a specific certificate fingerprint.
        Fingerprint(Vec<u8>),

        /// Verify with a specific PEM formatted CA.
        CaCert(X509),
    }

    fn fp_string(fp: &[u8]) -> String {
        use std::fmt::Write as _;

        let mut out = String::new();
        for b in fp {
            if !out.is_empty() {
                out.push(':');
            }
            let _ = write!(out, "{b:02x}");
        }
        out
    }

    fn verify_fingerprint(chain: &x509::X509StoreContextRef, expected_fingerprint: &[u8]) -> bool {
        let Some(cert) = chain.current_cert() else {
            log::error!("no certificate in chain?");
            return false;
        };

        let fp = match cert.digest(MessageDigest::sha256()) {
            Err(err) => {
                log::error!("error calculating certificate fingerprint: {err}");
                return false;
            }
            Ok(fp) => fp,
        };

        if expected_fingerprint != fp.as_ref() {
            log::error!("bad fingerprint: {}", fp_string(&fp));
            log::error!("expected fingerprint: {}", fp_string(&expected_fingerprint));
            return false;
        }

        true
    }

    impl<C, E> Client<C, E>
    where
        E: Environment,
        E::Error: From<anyhow::Error>,
    {
        /// Create a new client instance which will connect to the provided endpoint.
        pub fn with_options(
            api_url: Uri,
            environment: E,
            tls_options: TlsOptions,
            http_options: proxmox_http::HttpOptions,
        ) -> Result<HyperClient<E>, E::Error> {
            let mut connector = SslConnector::builder(SslMethod::tls_client())
                .map_err(|err| format_err!("failed to create ssl connector builder: {err}"))?;

            match tls_options {
                TlsOptions::Verify => (),
                TlsOptions::Insecure => connector.set_verify(SslVerifyMode::NONE),
                TlsOptions::Fingerprint(expected_fingerprint) => {
                    connector.set_verify_callback(SslVerifyMode::PEER, move |valid, chain| {
                        if valid {
                            return true;
                        }
                        verify_fingerprint(chain, &expected_fingerprint)
                    });
                }
                TlsOptions::CaCert(ca) => {
                    let mut store =
                        openssl::x509::store::X509StoreBuilder::new().map_err(|err| {
                            format_err!("failed to create certificate store builder: {err}")
                        })?;
                    store
                        .add_cert(ca)
                        .map_err(|err| format_err!("failed to build certificate store: {err}"))?;
                    connector.set_cert_store(store.build());
                }
            }

            let client = ProxmoxClient::with_ssl_connector(connector.build(), http_options);

            Ok(Client::with_client(api_url, environment, Arc::new(client)))
        }
    }

    impl super::HttpClient for Arc<proxmox_http::client::Client> {
        type Error = anyhow::Error;
        #[allow(clippy::type_complexity)]
        type Request =
            std::pin::Pin<Box<dyn Future<Output = Result<Response<Vec<u8>>, Self::Error>> + Send>>;

        fn request(&self, request: Request<Vec<u8>>) -> Self::Request {
            let (parts, body) = request.into_parts();
            let request = Request::<hyper::Body>::from_parts(parts, body.into());
            let this = Arc::clone(self);
            Box::pin(async move {
                use hyper::body::HttpBody;

                let (response, mut body) = (*this).request(request).await?.into_parts();

                let mut data = Vec::<u8>::new();
                while let Some(more) = body.data().await {
                    let more = more?;
                    data.extend(&more[..]);
                }

                Ok::<_, anyhow::Error>(Response::from_parts(response, data))
            })
        }
    }
}

#[cfg(feature = "hyper-client")]
pub use hyper_client_extras::TlsOptions;
