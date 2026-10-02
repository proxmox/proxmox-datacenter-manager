use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Error, bail};
use nix::unistd::User;
use serde::{Serialize, de::DeserializeOwned};

use pbs_api_types::Authid;
use proxmox_client::Client;
use proxmox_product_config::create_mocked_lock;
use proxmox_product_config::{ProductConfig, ProductConfigParams};
use proxmox_section_config::typed::SectionConfigData;

use pdm_api_types::ConfigDigest;
use pdm_api_types::remotes::{Remote, RemoteType};
use pdm_api_types::subscription::{SubscriptionKeyEntry, SubscriptionKeyShadow};
use pdm_config::remotes::RemoteConfig;
use pdm_config::subscriptions::SubscriptionKeyConfig;

use server::connection::{ClientFactory, PveClient};
use server::context::ContextFactory;
use server::pbs_client::PbsClient;

/// Shared, clonable storage for arbitrary data recorded by a [`PveTestRemote`]'s client
/// implementation.
#[derive(Clone, Default)]
pub struct TestRemoteState(Arc<Mutex<HashMap<String, serde_json::Value>>>);

impl TestRemoteState {
    /// Record `value` under `key`, overwriting any previous value stored there.
    pub fn set<T: Serialize>(&self, key: impl Into<String>, value: &T) {
        let value = serde_json::to_value(value).expect("failed to serialize test state value");
        self.0.lock().unwrap().insert(key.into(), value);
    }

    /// Retrieve the value previously stored under `key`, if any.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.0.lock().unwrap().get(key).cloned().map(|value| {
            serde_json::from_value(value).expect("failed to deserialize test state value")
        })
    }
}

/// A PVE remote registered in a [`TestApplication`], passed to the client factory whenever a
/// client for it is created.
#[derive(Clone)]
pub struct PveTestRemote {
    pub name: String,
    pub nodes: u32,
    pub state: TestRemoteState,
}

#[derive(Clone)]
struct PveTestRemoteWithClientMaker {
    remote: PveTestRemote,
    make_client: Arc<dyn Fn(PveTestRemote) -> Result<Arc<PveClient>, Error> + Send + Sync>,
}

impl PveTestRemote {
    /// Build the [`Remote`] configuration entry describing this test remote.
    pub fn to_remote(&self) -> Remote {
        Remote {
            ty: RemoteType::Pve,
            id: self.name.clone(),
            nodes: Vec::new(),
            authid: Authid::root_auth_id().clone(),
            token: "".into(),
            web_url: None,
        }
    }
}

/// A [`ContextFactory`] implementation for tests, backed by mocked remotes and a temporary
/// directory for config, caches and runtime directories.
///
/// Make sure to *not* drops this while running the test cast, otherwise the temporary directory
/// will be dropped.
#[derive(Clone)]
pub struct TestApplication {
    pve_remotes: HashMap<String, PveTestRemoteWithClientMaker>,
    base_dir: Arc<tempfile::TempDir>,
}

impl Default for TestApplication {
    fn default() -> Self {
        Self::new()
    }
}

impl TestApplication {
    /// Create a test application without any registered remotes.
    pub fn new() -> Self {
        let target_tmpdir = env!("CARGO_TARGET_TMPDIR");
        let base_dir = Arc::new(
            tempfile::tempdir_in(target_tmpdir).expect("could not create temporary directory"),
        );

        Self {
            pve_remotes: HashMap::new(),
            base_dir,
        }
    }

    /// Register a PVE remote called `name`, using `f` to create its client implementation.
    ///
    /// The client is constructed for every request, so any data that should outlive a single
    /// client has to be kept in `state`.
    pub fn with_pve_remote<S, C, F>(mut self, name: S, state: TestRemoteState, f: F) -> Self
    where
        S: Into<String>,
        C: pve_api_types::client::PveClient + Send + Sync + 'static,
        F: Fn(PveTestRemote) -> C + Send + Sync + 'static,
    {
        let name = name.into();
        let make_client = Arc::new(move |remote| Ok(Arc::new(f(remote)) as Arc<PveClient>));

        self.pve_remotes.insert(
            name.clone(),
            PveTestRemoteWithClientMaker {
                make_client,
                remote: PveTestRemote {
                    name,
                    nodes: 1,
                    state,
                },
            },
        );

        self
    }
}

impl ContextFactory for TestApplication {
    fn make_client_factory(&self) -> Result<Arc<dyn ClientFactory + Send + Sync>, Error> {
        Ok(Arc::new(self.clone()))
    }

    fn make_remote_config(&self) -> Result<Box<dyn RemoteConfig + Send + Sync>, Error> {
        Ok(Box::new(self.clone()))
    }

    fn make_subscription_key_config(
        &self,
    ) -> Result<Box<dyn SubscriptionKeyConfig + Send + Sync>, Error> {
        Ok(Box::new(TestSubscriptionKeyConfig::default()))
    }

    fn make_product_config(&self) -> Result<ProductConfig, Error> {
        let user = User::from_uid(nix::unistd::getuid())
            .ok()
            .flatten()
            .context("could not look up user")?;

        let base = self.base_dir.path();

        let config_dir = base.join("config");
        let state_dir = base.join("state");
        let run_dir = base.join("run");
        let cache_dir = base.join("cache");

        std::fs::create_dir(&config_dir)?;
        std::fs::create_dir(&state_dir)?;
        std::fs::create_dir(&run_dir)?;
        std::fs::create_dir(&cache_dir)?;

        std::fs::create_dir(run_dir.join("api-cache"))?;

        Ok(ProductConfig::new(ProductConfigParams {
            api_user: user.clone(),
            priv_user: user,
            config_dir,
            state_dir,
            run_dir,
            cache_dir,
        }))
    }

    // Override any other methods here if needed.
}

impl RemoteConfig for TestApplication {
    fn read(&self) -> Result<(SectionConfigData<Remote>, ConfigDigest), anyhow::Error> {
        let mut sections = SectionConfigData::default();

        for pve_remote in self.pve_remotes.values() {
            sections.insert(
                pve_remote.remote.name.clone(),
                pve_remote.remote.to_remote(),
            );
        }

        Ok((sections, ConfigDigest::from_slice([])))
    }

    fn read_secret_token(
        &self,
        _remote: &pdm_api_types::remotes::Remote,
    ) -> Result<String, anyhow::Error> {
        unimplemented!()
    }

    fn lock(&self) -> Result<proxmox_product_config::ApiLockGuard, anyhow::Error> {
        unimplemented!()
    }

    fn write(
        &self,
        _remotes: proxmox_section_config::typed::SectionConfigData<pdm_api_types::remotes::Remote>,
    ) -> Result<(), anyhow::Error> {
        unimplemented!()
    }
}

#[async_trait::async_trait]
impl ClientFactory for TestApplication {
    fn make_pve_client(&self, remote: &Remote) -> Result<Arc<PveClient>, Error> {
        if let Some(a) = self.pve_remotes.get(&remote.id) {
            (a.make_client)(a.remote.clone())
        } else {
            bail!("remote not registered")
        }
    }

    fn make_pve_client_with_endpoint(
        &self,
        _remote: &Remote,
        _target_endpoint: Option<&str>,
    ) -> Result<Arc<PveClient>, Error> {
        bail!("not implemented")
    }

    fn make_pbs_client(&self, _remote: &Remote) -> Result<Box<PbsClient>, Error> {
        bail!("not implemented")
    }

    fn make_raw_client(&self, _remote: &Remote) -> Result<Box<proxmox_client::Client>, Error> {
        bail!("not implemented")
    }

    async fn make_pve_client_and_login(&self, _remote: &Remote) -> Result<Arc<PveClient>, Error> {
        bail!("not implemented")
    }

    async fn make_pbs_client_and_login(
        &self,
        _remote: &Remote,
    ) -> Result<Box<PbsClient<Client>>, Error> {
        bail!("not implemented")
    }
}

#[derive(Default)]
struct TestSubscriptionKeyConfig {
    shadow: Mutex<SectionConfigData<SubscriptionKeyShadow>>,
    config: Mutex<SectionConfigData<SubscriptionKeyEntry>>,
}

impl SubscriptionKeyConfig for TestSubscriptionKeyConfig {
    fn read(&self) -> Result<(SectionConfigData<SubscriptionKeyEntry>, ConfigDigest), Error> {
        Ok((
            self.config.lock().unwrap().clone(),
            ConfigDigest::from_slice([]),
        ))
    }

    fn read_shadow(&self) -> Result<SectionConfigData<SubscriptionKeyShadow>, Error> {
        Ok(self.shadow.lock().unwrap().clone())
    }

    fn lock(&self) -> Result<proxmox_product_config::ApiLockGuard, Error> {
        Ok(unsafe { create_mocked_lock() })
    }

    fn write(
        &self,
        config: &SectionConfigData<SubscriptionKeyEntry>,
    ) -> Result<ConfigDigest, Error> {
        let mut guard = self.config.lock().unwrap();
        *guard = config.clone();

        Ok(ConfigDigest::from_slice([]))
    }

    fn write_shadow(&self, shadow: &SectionConfigData<SubscriptionKeyShadow>) -> Result<(), Error> {
        let mut guard = self.shadow.lock().unwrap();
        *guard = shadow.clone();

        Ok(())
    }
}

macro_rules! test_pve_client {
    ($ty:ident { $($overrides:tt)* }) => {

        pub struct $ty(crate::common::PveTestRemote);

        #[async_trait::async_trait]
        impl pve_api_types::client::PveClient for $ty {
            async fn list_nodes(
                &self,
            ) -> Result<Vec<pve_api_types::ClusterNodeIndexResponse>, proxmox_client::Error> {
                let mut nodes = Vec::new();

                for i in 0..self.0.nodes {
                    nodes.push(pve_api_types::ClusterNodeIndexResponse {
                        cpu: Some(0.0),
                        level: None,
                        maxcpu: Some(4),
                        maxmem: Some(4096),
                        mem: Some(1000),
                        node: format!("{}-node-{i}", self.0.name),
                        ssl_fingerprint: None,
                        status: pve_api_types::ClusterNodeIndexResponseStatus::Online,
                        uptime: Some(1000),
                    });
                }

                Ok(nodes)
            }
            $($overrides)*
        }

        impl $ty {
            /// Name of the remote this client was created for.
            pub fn remote(&self) -> &str {
                &self.0.name
            }

            /// Shared state of the remote this client was created for.
            pub fn state(&self) -> crate::common::TestRemoteState {
                self.0.state.clone()
            }
        }
    };
}

pub(crate) use test_pve_client;
