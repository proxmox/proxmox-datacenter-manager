use std::{
    collections::HashMap,
    fmt::Debug,
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Error;
use pdm_api_types::remotes::{Remote, RemoteType};
use pve_api_types::ClusterNodeIndexResponse;
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    task::JoinSet,
};

use crate::connection;

pub const DEFAULT_MAX_CONNECTIONS: usize = 20;
pub const DEFAULT_MAX_CONNECTIONS_PER_REMOTE: usize = 5;

pub struct ParallelFetcher<C> {
    pub max_connections: usize,
    pub max_connections_per_remote: usize,
    pub context: C,
}

pub struct FetchResults<T> {
    /// Per-remote results. The key in the map is the remote name.
    pub remote_results: HashMap<String, Result<RemoteResult<T>, Error>>,
}

impl<T> Default for FetchResults<T> {
    fn default() -> Self {
        Self {
            remote_results: Default::default(),
        }
    }
}

#[derive(Debug)]
pub struct RemoteResult<T> {
    /// Per-node results. The key in the map is the node name.
    pub node_results: HashMap<String, Result<NodeResults<T>, Error>>,
}

impl<T> Default for RemoteResult<T> {
    fn default() -> Self {
        Self {
            node_results: Default::default(),
        }
    }
}

#[derive(Debug)]
pub struct NodeResults<T> {
    /// The data returned from the passed function.
    pub data: T,
    /// Time needed waiting for the passed function to return.
    pub api_response_time: Duration,
}

impl<C: Clone + Send + 'static> ParallelFetcher<C> {
    pub fn new(context: C) -> Self {
        Self {
            max_connections: DEFAULT_MAX_CONNECTIONS,
            max_connections_per_remote: DEFAULT_MAX_CONNECTIONS_PER_REMOTE,
            context,
        }
    }

    pub async fn do_for_all_remote_nodes<A, F, T, Ft>(self, remotes: A, func: F) -> FetchResults<T>
    where
        A: Iterator<Item = Remote>,
        F: Fn(C, Remote, String) -> Ft + Clone + Send + 'static,
        Ft: Future<Output = Result<T, Error>> + Send + 'static,
        T: Send + Debug + 'static,
    {
        let total_connections_semaphore = Arc::new(Semaphore::new(self.max_connections));

        let mut remote_join_set = JoinSet::new();

        for remote in remotes {
            let semaphore = Arc::clone(&total_connections_semaphore);

            let f = func.clone();

            remote_join_set.spawn(Self::fetch_remote(
                remote,
                self.context.clone(),
                semaphore,
                f,
                self.max_connections_per_remote,
            ));
        }

        let mut results = FetchResults::default();

        while let Some(a) = remote_join_set.join_next().await {
            match a {
                Ok((remote_name, remote_result)) => {
                    results.remote_results.insert(remote_name, remote_result);
                }
                Err(err) => {
                    log::error!("join error when waiting for future: {err}")
                }
            }
        }

        results
    }

    async fn fetch_remote<F, Ft, T>(
        remote: Remote,
        context: C,
        semaphore: Arc<Semaphore>,
        func: F,
        max_connections_per_remote: usize,
    ) -> (String, Result<RemoteResult<T>, Error>)
    where
        F: Fn(C, Remote, String) -> Ft + Clone + Send + 'static,
        Ft: Future<Output = Result<T, Error>> + Send + 'static,
        T: Send + Debug + 'static,
    {
        let mut per_remote_results = RemoteResult::default();

        let mut permit = Some(Arc::clone(&semaphore).acquire_owned().await.unwrap());
        let per_remote_semaphore = Arc::new(Semaphore::new(max_connections_per_remote));

        match remote.ty {
            RemoteType::Pve => {
                let remote_clone = remote.clone();

                let nodes = match async move {
                    let client = connection::make_pve_client(&remote_clone)?;
                    let nodes = client.list_nodes().await?;

                    Ok::<Vec<ClusterNodeIndexResponse>, Error>(nodes)
                }
                .await
                {
                    Ok(nodes) => nodes,
                    Err(err) => return (remote.id.clone(), Err(err)),
                };

                let mut nodes_join_set = JoinSet::new();

                for node in nodes {
                    let permit = if let Some(permit) = permit.take() {
                        permit
                    } else {
                        Arc::clone(&semaphore).acquire_owned().await.unwrap()
                    };

                    let per_remote_connections_permit = Arc::clone(&per_remote_semaphore)
                        .acquire_owned()
                        .await
                        .unwrap();

                    let func_clone = func.clone();
                    let remote_clone = remote.clone();
                    let node_name = node.node.clone();
                    let context_clone = context.clone();

                    nodes_join_set.spawn(Self::fetch_node(
                        func_clone,
                        context_clone,
                        remote_clone,
                        node_name,
                        permit,
                        Some(per_remote_connections_permit),
                    ));
                }

                while let Some(join_result) = nodes_join_set.join_next().await {
                    match join_result {
                        Ok((node_name, per_node_result)) => {
                            per_remote_results
                                .node_results
                                .insert(node_name, per_node_result);
                        }
                        Err(e) => {
                            log::error!("join error when waiting for future: {e}")
                        }
                    }
                }
            }
            RemoteType::Pbs => {
                let (nodename, result) = Self::fetch_node(
                    func,
                    context,
                    remote.clone(),
                    "localhost".into(),
                    permit.unwrap(), // Always set to `Some` at this point
                    None,
                )
                .await;

                match result {
                    Ok(a) => per_remote_results.node_results.insert(nodename, Ok(a)),
                    Err(err) => per_remote_results.node_results.insert(nodename, Err(err)),
                };
            }
        }

        (remote.id, Ok(per_remote_results))
    }

    async fn fetch_node<F, Ft, T>(
        func: F,
        context: C,
        remote: Remote,
        node: String,
        _permit: OwnedSemaphorePermit,
        _per_remote_connections_permit: Option<OwnedSemaphorePermit>,
    ) -> (String, Result<NodeResults<T>, Error>)
    where
        F: Fn(C, Remote, String) -> Ft + Clone + Send + 'static,
        Ft: Future<Output = Result<T, Error>> + Send + 'static,
        T: Send + Debug + 'static,
    {
        let now = Instant::now();
        let result = func(context, remote.clone(), node.clone()).await;
        let api_response_time = now.elapsed();

        match result {
            Ok(data) => (
                node,
                Ok(NodeResults {
                    data,
                    api_response_time,
                }),
            ),
            Err(err) => (node, Err(err)),
        }
    }

    pub async fn do_for_all_remotes<A, F, T, Ft>(self, remotes: A, func: F) -> FetchResults<T>
    where
        A: Iterator<Item = Remote>,
        F: Fn(C, Remote, String) -> Ft + Clone + Send + 'static,
        Ft: Future<Output = Result<T, Error>> + Send + 'static,
        T: Send + Debug + 'static,
    {
        let total_connections_semaphore = Arc::new(Semaphore::new(self.max_connections));

        let mut node_join_set = JoinSet::new();
        let mut results = FetchResults::default();

        for remote in remotes {
            let total_connections_semaphore = total_connections_semaphore.clone();

            let remote_id = remote.id.clone();
            let context = self.context.clone();
            let func = func.clone();

            node_join_set.spawn(async move {
                let permit = total_connections_semaphore.acquire_owned().await.unwrap();

                (
                    remote_id,
                    Self::fetch_node(func, context, remote, "localhost".into(), permit, None).await,
                )
            });
        }

        while let Some(a) = node_join_set.join_next().await {
            match a {
                Ok((remote_id, (node_id, node_result))) => {
                    let mut node_results = HashMap::new();
                    node_results.insert(node_id, node_result);

                    let remote_result = RemoteResult { node_results };

                    if results
                        .remote_results
                        .insert(remote_id, Ok(remote_result))
                        .is_some()
                    {
                        // should never happen, but log for good measure if it actually does
                        log::warn!("made multiple requests for a remote!");
                    }
                }
                Err(err) => {
                    log::error!("join error when waiting for future: {err}")
                }
            }
        }

        results
    }
}
