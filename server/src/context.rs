//! Module to setup the API server's global runtime context.
//!
//! Make sure to call `init` *once* when starting up the API server.

use anyhow::Error;

use crate::connection;

/// Dependecy-inject production remote-config implementation and remote client factory
fn default_remote_setup() {
    pdm_config::remotes::init(Box::new(pdm_config::remotes::DefaultRemoteConfig));
    connection::init(Box::new(connection::DefaultClientFactory));
}

/// Dependency-inject concrete implementations needed at runtime.
pub fn init() -> Result<(), Error> {
    default_remote_setup();

    Ok(())
}
