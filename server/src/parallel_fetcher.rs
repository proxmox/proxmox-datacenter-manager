use std::fmt::Debug;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Error;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinSet;

use proxmox_log::LogContext;
use pve_api_types::ClusterNodeIndexResponse;

use pdm_api_types::remotes::{Remote, RemoteType};

use crate::connection;

pub const DEFAULT_MAX_CONNECTIONS: usize = 20;
pub const DEFAULT_MAX_CONNECTIONS_PER_REMOTE: usize = 5;

/// Response container type produced by [`ParallelFetcher::do_for_all_remotes`] or
/// [`ParallelFetcher::do_for_all_remote_nodes`].
///
/// This type contains the individual responses for each remote. These can be accessed
/// by iterating over this type (`.iter()`, `.into_iter()`) or by calling
/// [`FetcherResponse::get_remote_response`].
pub struct FetcherResponse<R> {
    // NOTE: This vector is sorted ascending by remote name.
    remote_responses: Vec<RemoteResponse<R>>,
}

impl<R> IntoIterator for FetcherResponse<R> {
    type Item = RemoteResponse<R>;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.remote_responses.into_iter()
    }
}

impl<'a, O> IntoIterator for &'a FetcherResponse<O> {
    type Item = &'a RemoteResponse<O>;
    type IntoIter = std::slice::Iter<'a, RemoteResponse<O>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<R> FetcherResponse<R> {
    /// Create a new iterator of all contained [`RemoteResponse`]s.
    pub fn iter<'a>(&'a self) -> std::slice::Iter<'a, RemoteResponse<R>> {
        self.remote_responses.iter()
    }

    /// Get the response for a particular remote.
    pub fn get_remote_response(&self, remote: &str) -> Option<&RemoteResponse<R>> {
        self.remote_responses
            .binary_search_by(|probe| probe.remote().cmp(remote))
            .ok()
            .map(|index| &self.remote_responses[index])
    }
}

/// Response container for one remote.
pub struct RemoteResponse<R> {
    remote_name: String,
    remote_type: RemoteType,
    response: R,
}

impl<R> RemoteResponse<R> {
    /// Returns the remote id.
    pub fn remote(&self) -> &str {
        self.remote_name.as_str()
    }

    /// Returns the type of the remote.
    pub fn remote_type(&self) -> RemoteType {
        self.remote_type
    }
}

impl<T> RemoteResponse<NodeResponse<T>> {
    /// Access the data that was returned by the handler function.
    pub fn data(&self) -> Result<&T, &Error> {
        self.response.data()
    }

    /// Access the data that was returned by the handler function, consuming self.
    pub fn into_data(self) -> Result<T, Error> {
        self.response.into_data()
    }

    /// The [`Duration`] of the handler call.
    pub fn handler_duration(&self) -> Duration {
        self.response.handler_duration()
    }
}

impl<T> RemoteResponse<MultipleNodesResponse<T>> {
    /// Access the node responses.
    ///
    /// This returns an error if the list of nodes could not be fetched
    /// during [`ParallelFetcher::do_for_all_remote_nodes`].
    pub fn nodes(&self) -> Result<&[NodeResponse<T>], &Error> {
        self.response.inner.as_ref().map(|inner| inner.as_slice())
    }

    /// Access the node responses, consuming self.
    ///
    /// This returns an error if the list of nodes could not be fetched
    /// during [`ParallelFetcher::do_for_all_remote_nodes`].
    pub fn into_nodes(self) -> Result<Vec<NodeResponse<T>>, Error> {
        self.response.inner
    }

    /// Access the remote name and node responses, consuming self.
    ///
    /// The node part is an error if the list of nodes could not be fetched
    /// during [`ParallelFetcher::do_for_all_remote_nodes`].
    pub fn into_remote_and_nodes(self) -> (String, Result<Vec<NodeResponse<T>>, Error>) {
        (self.remote_name, self.response.inner)
    }
}

/// Wrapper type used to contain the node responses when using
/// [`ParallelFetcher::do_for_all_remote_nodes`].
pub struct MultipleNodesResponse<T> {
    inner: Result<Vec<NodeResponse<T>>, Error>,
}

/// Response for a single node.
pub struct NodeResponse<T> {
    node_name: String,
    data: Result<T, Error>,
    api_response_time: Duration,
}

impl<T> NodeResponse<T> {
    /// Name of the node.
    ///
    /// At the moment, this is always `localhost` if `do_for_all_remotes` was used.
    /// If `do_for_all_remote_nodes` is used, this is the actual nodename for PVE remotes and
    /// `localhost` for PBS remotes.
    pub fn node_name(&self) -> &str {
        &self.node_name
    }

    /// Access the data that was returned by the handler function.
    pub fn data(&self) -> Result<&T, &Error> {
        self.data.as_ref()
    }

    /// Access the data that was returned by the handler function, consuming `self`.
    pub fn into_data(self) -> Result<T, Error> {
        self.data
    }

    /// The [`Duration`] of the handler call.
    pub fn handler_duration(&self) -> Duration {
        self.api_response_time
    }
}

/// Builder for the [`ParallelFetcher`] struct.
pub struct ParallelFetcherBuilder<C> {
    max_connections: Option<usize>,
    max_connections_per_remote: Option<usize>,
    context: C,
}

impl<C> ParallelFetcherBuilder<C> {
    fn new(context: C) -> Self {
        Self {
            context,
            max_connections: None,
            max_connections_per_remote: None,
        }
    }

    /// Set the maximum number of parallel connections.
    pub fn max_connections(mut self, limit: usize) -> Self {
        self.max_connections = Some(limit);
        self
    }

    /// Set the maximum number of parallel connections per remote.
    ///
    /// This only really affects PVE remotes with multiple cluster members.
    pub fn max_connections_per_remote(mut self, limit: usize) -> Self {
        self.max_connections_per_remote = Some(limit);
        self
    }

    /// Build the [`ParallelFetcher`] instance.
    pub fn build(self) -> ParallelFetcher<C> {
        ParallelFetcher {
            max_connections: self.max_connections.unwrap_or(DEFAULT_MAX_CONNECTIONS),
            max_connections_per_remote: self
                .max_connections_per_remote
                .unwrap_or(DEFAULT_MAX_CONNECTIONS_PER_REMOTE),
            context: self.context,
        }
    }
}

/// Helper for parallelizing API requests to multiple remotes/nodes.
pub struct ParallelFetcher<C> {
    max_connections: usize,
    max_connections_per_remote: usize,
    context: C,
}

impl<C: Clone + Send + 'static> ParallelFetcher<C> {
    /// Create a [`ParallelFetcher`] with default settings.
    pub fn new(context: C) -> Self {
        Self::builder(context).build()
    }

    /// Create the builder for constructing a [`ParallelFetcher`] with custom settings.
    pub fn builder(context: C) -> ParallelFetcherBuilder<C> {
        ParallelFetcherBuilder::new(context)
    }

    /// Invoke a function `func` for all nodes of a given list of remotes in parallel.
    pub async fn do_for_all_remote_nodes<A, F, T, Ft>(
        self,
        remotes: A,
        func: F,
    ) -> FetcherResponse<MultipleNodesResponse<T>>
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
            let future = Self::fetch_remote(
                remote,
                self.context.clone(),
                semaphore,
                f,
                self.max_connections_per_remote,
            );

            if let Some(log_context) = LogContext::current() {
                remote_join_set.spawn(log_context.scope(future));
            } else {
                remote_join_set.spawn(future);
            }
        }

        let mut remote_responses = Vec::new();

        while let Some(a) = remote_join_set.join_next().await {
            match a {
                Ok(remote_response) => remote_responses.push(remote_response),
                Err(err) => {
                    log::error!("join error when waiting for future: {err}")
                }
            }
        }

        remote_responses.sort_by(|a, b| a.remote().cmp(b.remote()));

        FetcherResponse { remote_responses }
    }

    async fn fetch_remote<F, Ft, T>(
        remote: Remote,
        context: C,
        semaphore: Arc<Semaphore>,
        func: F,
        max_connections_per_remote: usize,
    ) -> RemoteResponse<MultipleNodesResponse<T>>
    where
        F: Fn(C, Remote, String) -> Ft + Clone + Send + 'static,
        Ft: Future<Output = Result<T, Error>> + Send + 'static,
        T: Send + Debug + 'static,
    {
        let mut node_responses = Vec::new();

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
                    Err(err) => {
                        return RemoteResponse {
                            remote_name: remote.id,
                            remote_type: remote.ty,
                            response: MultipleNodesResponse { inner: Err(err) },
                        }
                    }
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

                    let future = Self::fetch_node(
                        func_clone,
                        context_clone,
                        remote_clone,
                        node_name,
                        permit,
                        Some(per_remote_connections_permit),
                    );

                    if let Some(log_context) = LogContext::current() {
                        nodes_join_set.spawn(log_context.scope(future));
                    } else {
                        nodes_join_set.spawn(future);
                    }
                }

                while let Some(join_result) = nodes_join_set.join_next().await {
                    match join_result {
                        Ok(node_response) => {
                            node_responses.push(node_response);
                        }
                        Err(e) => {
                            log::error!("join error when waiting for future: {e}")
                        }
                    }
                }
            }
            RemoteType::Pbs => {
                let node_response = Self::fetch_node(
                    func,
                    context,
                    remote.clone(),
                    "localhost".into(),
                    permit.unwrap(), // Always set to `Some` at this point
                    None,
                )
                .await;

                node_responses.push(node_response)
            }
        }

        RemoteResponse {
            remote_name: remote.id,
            remote_type: remote.ty,
            response: MultipleNodesResponse {
                inner: Ok(node_responses),
            },
        }
    }

    async fn fetch_node<F, Ft, T>(
        func: F,
        context: C,
        remote: Remote,
        node: String,
        _permit: OwnedSemaphorePermit,
        _per_remote_connections_permit: Option<OwnedSemaphorePermit>,
    ) -> NodeResponse<T>
    where
        F: Fn(C, Remote, String) -> Ft + Clone + Send + 'static,
        Ft: Future<Output = Result<T, Error>> + Send + 'static,
        T: Send + Debug + 'static,
    {
        let now = Instant::now();
        let result = func(context, remote.clone(), node.clone()).await;
        let api_response_time = now.elapsed();

        NodeResponse {
            node_name: node,
            data: result,
            api_response_time,
        }
    }

    /// Invoke a function `func` for all passed remotes in parallel.
    pub async fn do_for_all_remotes<A, F, T, Ft>(
        self,
        remotes: A,
        func: F,
    ) -> FetcherResponse<NodeResponse<T>>
    where
        A: Iterator<Item = Remote>,
        F: Fn(C, Remote, String) -> Ft + Clone + Send + 'static,
        Ft: Future<Output = Result<T, Error>> + Send + 'static,
        T: Send + Debug + 'static,
    {
        let total_connections_semaphore = Arc::new(Semaphore::new(self.max_connections));

        let mut node_join_set = JoinSet::new();

        for remote in remotes {
            let total_connections_semaphore = total_connections_semaphore.clone();

            let remote_id = remote.id.clone();
            let remote_type = remote.ty;

            let context = self.context.clone();
            let func = func.clone();
            let future = async move {
                let permit = total_connections_semaphore.acquire_owned().await.unwrap();

                RemoteResponse {
                    remote_type,
                    remote_name: remote_id,
                    response: Self::fetch_node(
                        func,
                        context,
                        remote,
                        "localhost".into(),
                        permit,
                        None,
                    )
                    .await,
                }
            };

            if let Some(log_context) = LogContext::current() {
                node_join_set.spawn(log_context.scope(future));
            } else {
                node_join_set.spawn(future);
            }
        }

        let mut remote_responses = Vec::new();

        while let Some(a) = node_join_set.join_next().await {
            match a {
                Ok(remote_response) => remote_responses.push(remote_response),
                Err(err) => {
                    log::error!("join error when waiting for future: {err}")
                }
            }
        }

        remote_responses.sort_by(|a, b| a.remote().cmp(b.remote()));

        FetcherResponse { remote_responses }
    }
}
