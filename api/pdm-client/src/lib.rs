//! Proxmox Datacenter Manager API client.

use anyhow::Error;
use hyper::client::Client as HyperClient;

use proxmox_http::client::HttpsConnector;

use pdm_api_types::Authid;

pub struct Client {
    _options: Options,
    _client: HyperClient<HttpsConnector>,
}

impl Client {
    pub fn new(server: &str, port: u16, auth_id: &Authid, options: Options) -> Result<Self, Error> {
        let _ = (server, port, auth_id, options);
        todo!();
    }
}

#[derive(Default)]
// TODO: Merge this with pbs-client's stuff
pub struct Options {
    /// XDG base directory prefix for storing the cached ticket.
    prefix: Option<String>,

    /// Certificate fingerprint.
    fingerprint: Option<String>,
}

impl Options {
    /// New default instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// XDG base directory prefix for storing the cached ticket.
    pub fn prefix(mut self, prefix: String) -> Self {
        self.prefix = Some(prefix);
        self
    }

    /// Certificate fingerprint.
    pub fn fingerprint(mut self, fingerprint: String) -> Self {
        self.fingerprint = Some(fingerprint);
        self
    }
}
