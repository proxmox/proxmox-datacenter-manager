//! Module to setup the API server's global runtime context.
//!
//! Make sure to call `init` *once* when starting up the API server.

use std::sync::{Arc, OnceLock};

use anyhow::Error;

use proxmox_product_config::{ProductConfig, ProductConfigParams};

use pdm_config::{remotes::RemoteConfig, subscriptions::SubscriptionKeyConfig};

use crate::api_cache::ApiCache;
use crate::connection::{self, ClientFactory};

#[cfg(remote_config = "faked")]
mod faked_remotes;

#[cfg(not(remote_config = "faked"))]
mod default;

static APP: OnceLock<PdmApplication> = OnceLock::new();

/// Dependency-inject concrete implementations needed at runtime.
pub fn init() -> Result<PdmApplication, Error> {
    let factory = context_factory()?;

    let app = factory.make_pdm_application()?;
    APP.set(app.clone())
        .map_err(|_| anyhow::format_err!("context::init was already called"))?;

    // The application handle and legacy accessors use separate remote-config instances. Factories
    // must provide equivalent views: the default implementation is stateless, and the fake
    // implementation clones the same data.
    pdm_config::remotes::init(factory.make_remote_config()?);
    pdm_config::subscriptions::init(factory.make_subscription_key_config()?);

    Ok(app)
}

/// Retrieve a handle to [`PdmApplication`] for this server.
///
/// Prefer to retrieve this via the API handler using `#[state] app: PdmApplication` in the
/// API handler signature.
pub fn pdm_application() -> PdmApplication {
    APP.get()
        .expect("context::init was not called to set up the application context object")
        .clone()
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

    fn make_product_config(&self) -> Result<ProductConfig, Error> {
        let product_config = ProductConfig::new(ProductConfigParams {
            api_user: pdm_config::api_user()?,
            priv_user: pdm_config::priv_user()?,
            config_dir: pdm_buildcfg::configdir!("/").into(),
            state_dir: pdm_buildcfg::statedir!("/").into(),
            run_dir: pdm_buildcfg::rundir!("/").into(),
            cache_dir: pdm_buildcfg::PDM_CACHE_DIR.into(),
        });

        Ok(product_config)
    }

    fn make_pdm_application(&self) -> Result<PdmApplication, Error> {
        let product_config = self.make_product_config()?;

        let api_cache = ApiCache::new(
            product_config.run_dir().join("api-cache"),
            product_config.default_dir_create_options(),
            product_config.default_file_create_options(),
        );

        Ok(PdmApplication {
            inner: Arc::new(PdmApplicationInner {
                client_factory: self.make_client_factory()?,
                remote_config: self.make_remote_config()?,
                subscription_key_config: self.make_subscription_key_config()?,
                product_config,

                api_cache,
            }),
        })
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

/// Application context handle.
///
/// This type gives access to dependency-injected implementations and general product
/// configuration.
///
/// This implements [`Clone`] and can be cheaply copied (it contains a single `Arc`).
#[derive(Clone)]
pub struct PdmApplication {
    inner: Arc<PdmApplicationInner>,
}

impl PdmApplication {
    /// Get a handle to the [`ClientFactory`] as an [`Arc`].
    ///
    /// Prefer to use [`Self::client_factory`] if possible.
    pub fn client_factory_shared(&self) -> Arc<dyn ClientFactory + Send + Sync> {
        Arc::clone(&self.inner.client_factory)
    }

    /// Get a reference to the [`ClientFactory`].
    pub fn client_factory(&self) -> &(dyn ClientFactory + Send + Sync) {
        self.inner.client_factory.as_ref()
    }

    /// Get a reference to the [`RemoteConfig`].
    pub fn remote_config(&self) -> &(dyn RemoteConfig + Send + Sync) {
        self.inner.remote_config.as_ref()
    }

    /// Get a reference to the [`SubscriptionKeyConfig`].
    pub fn subscription_key_config(&self) -> &(dyn SubscriptionKeyConfig + Send + Sync) {
        self.inner.subscription_key_config.as_ref()
    }

    /// Get a reference to the [`ProductConfig`].
    pub fn product_config(&self) -> &ProductConfig {
        &self.inner.product_config
    }

    /// Get a reference to the [`ApiCache`].
    pub fn api_cache(&self) -> &ApiCache {
        &self.inner.api_cache
    }
}

struct PdmApplicationInner {
    client_factory: Arc<dyn ClientFactory + Send + Sync>,
    remote_config: Box<dyn RemoteConfig + Send + Sync>,
    subscription_key_config: Box<dyn SubscriptionKeyConfig + Send + Sync>,
    product_config: ProductConfig,

    api_cache: ApiCache,
}
