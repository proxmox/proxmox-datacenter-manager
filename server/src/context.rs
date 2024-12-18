//! Module to setup the API server's global runtime context.
//!
//! Make sure to call `init` *once* when starting up the API server.

use anyhow::Error;

use crate::connection;

/// Dependency-inject production remote-config implementation and remote client factory
#[allow(dead_code)]
fn default_remote_setup() {
    pdm_config::remotes::init(Box::new(pdm_config::remotes::DefaultRemoteConfig));
    connection::init(Box::new(connection::DefaultClientFactory));
}

/// Dependency-inject concrete implementations needed at runtime.
pub fn init() -> Result<(), Error> {
    #[cfg(remote_config = "faked")]
    {
        use anyhow::bail;

        use crate::test_support::fake_remote;

        match std::env::var("PDM_FAKED_REMOTE_CONFIG") {
            Ok(path) => {
                log::info!("using fake remotes from {path:?}");
                let config = fake_remote::FakeRemoteConfig::from_json_config(&path)?;
                pdm_config::remotes::init(Box::new(config.clone()));
                connection::init(Box::new(fake_remote::FakeClientFactory { config }));
            }
            Err(_) => {
                bail!("compiled with remote_config = 'faked', but PDM_FAKED_REMOTE_CONFIG not set")
            }
        }
    }
    #[cfg(not(remote_config = "faked"))]
    {
        default_remote_setup();
    }

    Ok(())
}
