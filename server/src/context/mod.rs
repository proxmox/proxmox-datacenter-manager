//! Module to setup the API server's global runtime context.
//!
//! Make sure to call `init` *once* when starting up the API server.

use std::sync::Arc;

use anyhow::Error;
use pdm_config::{remotes::RemoteConfig, subscriptions::SubscriptionKeyConfig};

use crate::connection::{self, ClientFactory};

#[cfg(remote_config = "faked")]
mod faked_remotes;

#[cfg(not(remote_config = "faked"))]
mod default;

/// Dependency-inject concrete implementations needed at runtime.
pub fn init() -> Result<(), Error> {
    let factory = context_factory()?;

    pdm_config::subscriptions::init(factory.make_subscription_key_config()?);
    pdm_config::remotes::init(factory.make_remote_config()?);
    // FIXME: Rather let connection use an Application context object from here
    connection::init(factory.make_client_factory()?);

    Ok(())
}

pub trait ContextFactory {
    fn make_client_factory(&self) -> Result<Arc<dyn ClientFactory + Send + Sync>, Error> {
        Ok(Arc::new(connection::DefaultClientFactory))
    }

    fn make_remote_config(&self) -> Result<Box<dyn RemoteConfig + Send + Sync>, Error> {
        Ok(Box::new(pdm_config::remotes::DefaultRemoteConfig))
    }

    fn make_subscription_key_config(
        &self,
    ) -> Result<Box<dyn SubscriptionKeyConfig + Send + Sync>, Error> {
        Ok(Box::new(
            pdm_config::subscriptions::DefaultSubscriptionKeyConfig,
        ))
    }
}

fn context_factory() -> Result<impl ContextFactory, Error> {
    #[cfg(remote_config = "faked")]
    {
        faked_remotes::FakedRemoteContextFactory::new()
    }
    #[cfg(not(remote_config = "faked"))]
    {
        Ok(default::DefaultContextFactory)
    }
}
