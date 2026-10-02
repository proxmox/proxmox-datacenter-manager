use std::sync::Arc;

use anyhow::{Context, Error};
use pdm_config::remotes::RemoteConfig;

use crate::connection::ClientFactory;
use crate::context::ContextFactory;
use crate::test_support::fake_remote::{FakeClientFactory, FakeRemoteConfig};

pub struct FakedRemoteContextFactory(FakeRemoteConfig);

impl FakedRemoteContextFactory {
    pub fn new() -> Result<Self, Error> {
        let path = std::env::var("PDM_FAKED_REMOTE_CONFIG").context(
            "compiled with remote_config = 'faked', but PDM_FAKED_REMOTE_CONFIG not set",
        )?;

        log::info!("using fake remotes from {path:?}");
        let config = FakeRemoteConfig::from_json_config(&path)
            .context("could not deserialize fake remote config")?;

        Ok(Self(config))
    }
}

impl ContextFactory for FakedRemoteContextFactory {
    fn make_client_factory(&self) -> Result<Arc<dyn ClientFactory + Send + Sync>, Error> {
        Ok(Arc::new(FakeClientFactory {
            config: self.0.clone(),
        }))
    }

    fn make_remote_config(&self) -> Result<Box<dyn RemoteConfig + Send + Sync>, Error> {
        Ok(Box::new(self.0.clone()))
    }

    // No need to override subscription_key_config_impl here.
    //
    // The subscription key pool is product-only (PDM stores its own pool of
    // keys regardless of how remotes are mocked or not), so initialise it on
    // both paths.
}
