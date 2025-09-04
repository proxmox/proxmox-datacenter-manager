use std::error::Error as StdError;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{self, bail, Context};

use futures::{future::join_all, stream::FuturesUnordered, StreamExt, TryFutureExt};
use pdm_api_types::{remotes::Remote, RemoteUpid};
use pve_api_types::{
    client::PveClient, CreateSdnLock, CreateVnet, CreateZone, PveUpid, ReleaseSdnLock, ReloadSdn,
    RollbackSdn,
};

use crate::api::pve::{connect, get_remote};

/// Wrapper for [`PveClient`] for representing a locked SDN configuration.
///
/// It stores the client that has been locked, as well as the lock_token that is required for
/// making changes to the SDN configuration. It provides methods that proxy the respective SDN
/// endpoints, where it adds the lock_token when making the proxied calls.
pub struct LockedSdnClient {
    lock_token: String,
    client: Arc<dyn PveClient + Send + Sync>,
}

#[derive(Debug)]
pub enum LockedSdnClientError {
    Client(proxmox_client::Error),
    Other(anyhow::Error),
}

impl StdError for LockedSdnClientError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Client(cli) => Some(cli),
            Self::Other(_) => None, // anyhow is not a std error
        }
    }
}

impl std::fmt::Display for LockedSdnClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Client(err) => err.fmt(f),
            Self::Other(err) => err.fmt(f),
        }
    }
}

impl From<proxmox_client::Error> for LockedSdnClientError {
    fn from(value: proxmox_client::Error) -> Self {
        Self::Client(value)
    }
}

impl From<anyhow::Error> for LockedSdnClientError {
    fn from(value: anyhow::Error) -> Self {
        Self::Other(value)
    }
}

impl LockedSdnClient {
    /// Creates a new PveClient for a given [`Remote`] and locks the SDN configuration there.
    ///
    /// # Errors
    ///
    /// This function will return an error if locking the remote fails.
    pub async fn new(
        remote: &Remote,
        allow_pending: impl Into<Option<bool>>,
    ) -> Result<Self, LockedSdnClientError> {
        let client = connect(remote)?;

        let params = CreateSdnLock {
            allow_pending: allow_pending.into(),
        };

        client
            .acquire_sdn_lock(params)
            .await
            .map(|lock_token| Self { lock_token, client })
            .map_err(LockedSdnClientError::from)
    }

    /// proxies [`PveClient::create_vnet`] and adds lock_token to the passed parameters before
    /// making the call.
    pub async fn create_vnet(&self, mut params: CreateVnet) -> Result<(), proxmox_client::Error> {
        params.lock_token = Some(self.lock_token.clone());

        self.client.create_vnet(params).await
    }

    /// proxies [`PveClient::create_zone`] and adds lock_token to the passed parameters before
    /// making the call.
    pub async fn create_zone(&self, mut params: CreateZone) -> Result<(), proxmox_client::Error> {
        params.lock_token = Some(self.lock_token.clone());

        self.client.create_zone(params).await
    }

    /// applies the changes made while the client was locked and returns the original [`PveClient`] if the
    /// changes have been applied successfully.
    pub async fn apply_and_release(
        self,
    ) -> Result<(PveUpid, Arc<dyn PveClient + Send + Sync>), proxmox_client::Error> {
        let params = ReloadSdn {
            lock_token: Some(self.lock_token.clone()),
            release_lock: Some(true),
        };

        self.client
            .sdn_apply(params)
            .await
            .map(move |upid| (upid, self.client))
    }

    /// releases the lock on the [`PveClient`] without applying pending changes.
    pub async fn release(
        self,
        force: impl Into<Option<bool>>,
    ) -> Result<Arc<dyn PveClient + Send + Sync>, proxmox_client::Error> {
        let params = ReleaseSdnLock {
            force: force.into(),
            lock_token: Some(self.lock_token),
        };

        self.client.release_sdn_lock(params).await?;
        Ok(self.client)
    }

    /// rolls back all pending changes and then releases the lock
    pub async fn rollback_and_release(
        self,
    ) -> Result<Arc<dyn PveClient + Send + Sync>, proxmox_client::Error> {
        let params = RollbackSdn {
            lock_token: Some(self.lock_token),
            release_lock: Some(true),
        };

        self.client.rollback_sdn_changes(params).await?;
        Ok(self.client)
    }
}

/// Context for [`LockedSdnClient`] stored in [`LockedSdnClients`].
pub struct LockedSdnClientContext<C> {
    remote_id: String,
    data: C,
}

impl<C> LockedSdnClientContext<C> {
    fn new(remote_id: String, data: C) -> Self {
        Self { remote_id, data }
    }

    pub fn remote_id(&self) -> &str {
        &self.remote_id
    }

    pub fn data(&self) -> &C {
        &self.data
    }
}

/// A collection abstracting [`LockedSdnClient`] for multiple locked remotes.
///
/// It can be used for running the same command across multiple remotes, while automatically
/// handling rollback and releasing locks in case of failures across all remotes. If an API call
/// made to one of the remotes fails, then this client will automatically take care of rolling back
/// all changes made during the transaction and then releasing the locks.
pub struct LockedSdnClients<C> {
    clients: Vec<(LockedSdnClient, LockedSdnClientContext<C>)>,
}

impl<C> LockedSdnClients<C> {
    /// A convenience function for creating locked clients for multiple remotes.
    ///
    /// For each remote a Context can be specified, which will be supplied to all callbacks that
    /// are using this [`LockedSdnClients`] to make calls across all remotes.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// * the remote configuration cannot be read
    /// * any of the supplied remotes is not contained in the configuration
    /// * locking the configuration on any remote fails
    ///
    /// If necessary, the configuration of all remotes will be unlocked, if possible.
    pub async fn from_remote_names<I: IntoIterator<Item = (String, C)>>(
        remote_names: I,
        allow_pending: bool,
    ) -> Result<Self, anyhow::Error> {
        let (remote_config, _) = pdm_config::remotes::config()?;

        let mut clients = Vec::new();

        for (remote_name, context) in remote_names {
            let remote = get_remote(&remote_config, &remote_name)?;
            proxmox_log::info!("acquiring lock for remote {}", remote.id);

            match LockedSdnClient::new(remote, allow_pending).await {
                Ok(client) => {
                    let context = LockedSdnClientContext::new(remote_name, context);
                    clients.push((client, context));
                }
                Err(error) => {
                    proxmox_log::info!(
                        "encountered an error when locking a remote, releasing all locks"
                    );

                    for (client, ctx) in clients {
                        proxmox_log::info!("releasing lock for remote {}", ctx.remote_id);

                        if let Err(error) = client.release(false).await {
                            proxmox_log::error!(
                                "could not release lock for remote {}: {error:#}",
                                remote.id
                            )
                        }
                    }

                    return match &error {
                        LockedSdnClientError::Client(proxmox_client::Error::Api(status, _msg))
                            if *status == 501 =>
                        {
                            bail!("remote {} does not support the sdn locking api, please upgrade to PVE 9 or newer!", remote.id)
                        }
                        _ => Err(error).with_context(|| {
                            format!("could not lock sdn configuration for remote {}", remote.id)
                        }),
                    };
                }
            };
        }

        clients.sort_by(|(_, ctx_a), (_, ctx_b)| ctx_a.remote_id.cmp(&ctx_b.remote_id));

        Ok(Self { clients })
    }

    /// Executes the given callback for each [`LockedSdnClient`] in this collection.
    ///
    /// On error, it tries to rollback the configuration of *all* locked clients, releases the lock
    /// and returns the error. If rollbacking fails, an error will be logged and no further action
    /// is taken.
    pub async fn for_each<F>(self, callback: F) -> Result<Self, anyhow::Error>
    where
        F: AsyncFn(
            &LockedSdnClient,
            &LockedSdnClientContext<C>,
        ) -> Result<(), proxmox_client::Error>,
    {
        let futures = self.clients.iter().map(|(client, context)| {
            callback(client, context)
                .map_ok(|_| context.remote_id())
                .map_err(|err| (err, context.remote_id()))
        });

        let mut errors = false;

        for result in join_all(futures).await {
            match result {
                Ok(remote_id) => {
                    proxmox_log::info!("succcessfully executed transaction on remote {remote_id}");
                }
                Err((error, remote_id)) => {
                    proxmox_log::error!(
                        "failed to execute transaction on remote {remote_id}: {error:#}",
                    );
                    errors = true;
                }
            }
        }

        if errors {
            let mut rollback_futures = FuturesUnordered::new();

            for (client, ctx) in self.clients {
                let ctx = Arc::new(ctx);
                let err_ctx = ctx.clone();

                rollback_futures.push(
                    client
                        .rollback_and_release()
                        .map_ok(|_| ctx)
                        .map_err(|err| (err, err_ctx)),
                );
            }

            while let Some(result) = rollback_futures.next().await {
                match result {
                    // older versions of PVE 9 potentially return 1 instead of an empty body, which
                    // can trigger an BadApi Error in the client. Ignore the error here to work around
                    // this issue.
                    Ok(ctx) | Err((proxmox_client::Error::BadApi(_, _), ctx)) => {
                        proxmox_log::info!(
                            "successfully rolled back configuration for remote {}",
                            ctx.remote_id()
                        )
                    }
                    Err((_, ctx)) => {
                        proxmox_log::error!(
                            "could not rollback and unlock configuration for remote {} - configuration needs to be manually unlocked via 'pvesh delete /cluster/sdn/lock --force 1'",
                            ctx.remote_id()
                        )
                    }
                }
            }

            bail!("running the transaction failed on at least one remote!");
        }

        Ok(self)
    }

    // pve-http-server TCP connection timeout is 5 seconds, use a lower amount with some margin for
    // latency in order to avoid re-opening TCP connections for every polling request.
    const POLLING_INTERVAL: Duration = Duration::from_secs(3);

    /// Convenience function for polling a running task on a PVE remote.
    ///
    /// It polls a given task on a given node, waiting for the task to finish successfully.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// * There was a problem querying the task status (this does not necessarily mean the task failed).
    /// * The task finished unsuccessfully.
    async fn poll_task(
        node: String,
        upid: RemoteUpid,
        client: Arc<dyn PveClient + Send + Sync>,
    ) -> Result<RemoteUpid, anyhow::Error> {
        loop {
            tokio::time::sleep(Self::POLLING_INTERVAL).await;

            let status = client.get_task_status(&node, &upid.upid).await?;

            if !status.is_running() {
                if status.finished_successfully() == Some(true) {
                    return Ok(upid);
                } else {
                    bail!(
                        "task did not finish successfully on remote {}",
                        upid.remote()
                    );
                }
            }
        }
    }

    /// Applies and Reloads the SDN configuration for all locked clients.
    ///
    /// This function tries to apply the SDN configuration for all supplied locked clients and, if
    /// it was successful, to reload the SDN configuration of the remote. It logs success and error
    /// messages via proxmox_log. Rollbacking in cases of failure is no longer possible, so this
    /// function then returns an error if applying or reloading the configuration was unsuccessful
    /// on at least one remote.
    ///
    /// # Errors This function returns an error if applying or reloading the configuration on one
    /// of the remotes failed. It will always wait for all futures to finish and only return an
    /// error afterwards.
    pub async fn apply_and_release(self) -> Result<(), anyhow::Error> {
        let mut futures = FuturesUnordered::new();

        for (client, context) in self.clients {
            let ctx = Arc::new(context);
            let err_ctx = ctx.clone();

            futures.push(
                client
                    .apply_and_release()
                    .map_ok(|(upid, client)| ((upid, client), ctx))
                    .map_err(|err| (err, err_ctx)),
            );
        }

        let mut reload_futures = FuturesUnordered::new();

        while let Some(result) = futures.next().await {
            match result {
                Ok(((upid, client), ctx)) => {
                    proxmox_log::info!(
                        "successfully applied sdn config on remote {}",
                        ctx.remote_id()
                    );

                    let Ok(remote_upid) =
                        RemoteUpid::try_from((ctx.remote_id(), upid.to_string().as_str()))
                    else {
                        proxmox_log::error!("invalid UPID received from PVE: {upid}");
                        continue;
                    };

                    reload_futures.push(
                        Self::poll_task(upid.node.clone(), remote_upid, client)
                            .map_err(move |err| (err, ctx)),
                    );
                }
                Err((error, ctx)) => {
                    proxmox_log::error!(
                        "failed to apply sdn configuration on remote {}: {error:#}, not reloading",
                        ctx.remote_id()
                    );
                }
            }
        }

        proxmox_log::info!(
            "Waiting for reload tasks to finish on all remotes, this can take awhile"
        );

        let mut errors = false;

        while let Some(result) = reload_futures.next().await {
            match result {
                Ok(upid) => {
                    proxmox_log::info!(
                        "successfully reloaded configuration on remote {}",
                        upid.remote()
                    );
                }
                Err((error, ctx)) => {
                    proxmox_log::error!(
                        "could not reload configuration on remote {}: {error:#}",
                        ctx.remote_id()
                    );

                    errors = true;
                }
            }
        }

        if errors {
            bail!("failed to apply configuration on at least one remote");
        }

        Ok(())
    }
}
