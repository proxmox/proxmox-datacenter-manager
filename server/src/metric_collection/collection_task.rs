use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Error;
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        oneshot, OwnedSemaphorePermit, Semaphore,
    },
    task::JoinSet,
    time::{Interval, MissedTickBehavior},
};

use proxmox_section_config::typed::SectionConfigData;
use proxmox_sys::fs::CreateOptions;

use pdm_api_types::remotes::{Remote, RemoteType};

use crate::metric_collection::rrd_task::CollectionStats;
use crate::{connection, task_utils};

use super::{
    rrd_task::{RrdStoreRequest, RrdStoreResult},
    state::{MetricCollectionState, RemoteStatus},
};

/// Location of the metric collection state file.
const METRIC_COLLECTION_STATE_FILE: &str = concat!(
    pdm_buildcfg::PDM_STATE_DIR_M!(),
    "/metric-collection-state.json"
);

pub const MAX_CONCURRENT_CONNECTIONS: usize = 20;

/// Default metric collection interval.
pub const DEFAULT_COLLECTION_INTERVAL: u64 = 600;
/// Minimum metric collection interval.
pub const MIN_COLLECTION_INTERVAL: u64 = 10;

/// Control messages for the metric collection task.
pub(super) enum ControlMsg {
    TriggerMetricCollection(Option<String>, oneshot::Sender<()>),
}

/// Task which periodically collects metrics from all remotes and stores
/// them in the local metrics database.
pub(super) struct MetricCollectionTask {
    state: MetricCollectionState,
    metric_data_tx: Sender<RrdStoreRequest>,
    control_message_rx: Receiver<ControlMsg>,
}

impl MetricCollectionTask {
    /// Create a new metric collection task.
    pub(super) fn new(
        metric_data_tx: Sender<RrdStoreRequest>,
        control_message_rx: Receiver<ControlMsg>,
    ) -> Result<Self, Error> {
        let state = load_state()?;

        Ok(Self {
            state,
            metric_data_tx,
            control_message_rx,
        })
    }

    /// Run the metric collection task.
    ///
    /// This function never returns.
    #[tracing::instrument(skip_all, name = "metric_collection_task")]
    pub(super) async fn run(&mut self) {
        let (mut timer, first_tick) = Self::setup_timer(DEFAULT_COLLECTION_INTERVAL);

        log::debug!(
            "metric collection starting up. Collection interval set to {} seconds.",
            DEFAULT_COLLECTION_INTERVAL,
        );
        // Check and fetch any remote which would be overdue by the time the
        // timer first fires.
        if let Some(remote_config) = Self::load_remote_config() {
            self.fetch_overdue(&remote_config, first_tick, DEFAULT_COLLECTION_INTERVAL)
                .await;
        }

        loop {
            tokio::select! {
                _ = timer.tick() => {
                    self.handle_tick().await;
                }

                Some(message) = self.control_message_rx.recv() => {
                    self.handle_control_message(message).await;
                }
            }

            if let Err(err) = self.state.save() {
                log::error!("could not update metric collection state: {err}");
            }
        }
    }

    /// Handle a timer tick.
    async fn handle_tick(&mut self) {
        log::debug!("starting metric collection from all remotes - triggered by timer");

        if let Some(remotes) = Self::load_remote_config() {
            self.cleanup_removed_remotes_from_state(&remotes);

            let now = Instant::now();
            let to_fetch = remotes
                .iter()
                .map(|(name, _)| name.into())
                .collect::<Vec<String>>();
            self.fetch_remotes(&remotes, &to_fetch).await;
            let elapsed = now.elapsed();

            if let Err(err) = self
                .metric_data_tx
                .send(RrdStoreRequest::CollectionStats {
                    timestamp: proxmox_time::epoch_i64(),
                    stats: CollectionStats {
                        // TODO: use as_millis_f64 once stabilized
                        total_time: elapsed.as_secs_f64() * 1000.,
                    },
                })
                .await
            {
                log::error!("could not send collection stats to rrd task: {err}");
            }
        }
    }

    /// Handle a control message for force-triggered collection.
    async fn handle_control_message(&mut self, message: ControlMsg) {
        if let Some(remotes) = Self::load_remote_config() {
            let done_tx = match message {
                ControlMsg::TriggerMetricCollection(Some(remote), done_tx) => {
                    log::debug!("starting metric collection for remote '{remote}'- triggered by control message");
                    self.fetch_remotes(&remotes, &[remote]).await;
                    done_tx
                }
                ControlMsg::TriggerMetricCollection(None, done_tx) => {
                    log::debug!("starting metric collection from all remotes - triggered by control message");
                    let to_fetch = remotes
                        .iter()
                        .map(|(name, _)| name.into())
                        .collect::<Vec<String>>();
                    self.fetch_remotes(&remotes, &to_fetch).await;
                    done_tx
                }
            };

            // We don't care about the result, if the caller does not wait for the result, it
            // might have dropped the receiver already.
            let _ = done_tx.send(());
        }
    }

    fn cleanup_removed_remotes_from_state(&mut self, remotes: &SectionConfigData<Remote>) {
        self.state.retain(|remote| remotes.get(remote).is_some());
    }

    /// Set up a [`tokio::time::Interval`] instance with the provided interval.
    /// The timer will be aligned, e.g. an interval of `60` will let the timer
    /// fire at minute boundaries.
    ///
    /// The return values are a tuple of the [`tokio::time::Interval`] timer instance
    /// and the [`std::time::Instant`] at which the timer first fires.
    fn setup_timer(interval: u64) -> (Interval, Instant) {
        log::debug!("setting metric collection interval timer to {interval} seconds.",);
        let mut timer = tokio::time::interval(Duration::from_secs(interval));

        // If we miss a tick because a previous collection run took too long, we want to
        // tick as soon as possible, but we do not need to repeat missing ticks.
        // We do want to stay aligned, though.
        // https://docs.rs/tokio/latest/tokio/time/enum.MissedTickBehavior.html#variant.Skip
        timer.set_missed_tick_behavior(MissedTickBehavior::Skip);

        let first_run = task_utils::next_aligned_instant(interval);
        timer.reset_at(first_run.into());

        (timer, first_run)
    }

    /// Convenience helper to load `remote.cfg`, logging the error
    /// and returning `None` if the config could not be read.
    fn load_remote_config() -> Option<SectionConfigData<Remote>> {
        match pdm_config::remotes::config() {
            Ok((remotes, _)) => Some(remotes),
            Err(e) => {
                log::error!("failed to collect metrics, could not read remotes.cfg: {e}");
                None
            }
        }
    }

    /// Fetch metric data from a provided list of remotes concurrently.
    /// The maximum number of concurrent connections is determined by
    /// `max_concurrent_connections` in the [`CollectionSettings`]
    /// instance in `self`.
    async fn fetch_remotes(
        &mut self,
        remote_config: &SectionConfigData<Remote>,
        remotes_to_fetch: &[String],
    ) {
        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_CONNECTIONS));
        let mut handles = JoinSet::new();

        let now = proxmox_time::epoch_i64();

        for remote_name in remotes_to_fetch {
            let status = self
                .state
                .get_status(remote_name)
                .cloned()
                .unwrap_or_default();

            if now - status.last_collection.unwrap_or(0) < MIN_COLLECTION_INTERVAL as i64 {
                log::debug!(
                    "skipping metric collection for remote '{remote_name}' - data is recent enough"
                );
                continue;
            }

            // unwrap is okay here, acquire_* will only fail if `close` has been
            // called on the semaphore.
            let permit = Arc::clone(&semaphore).acquire_owned().await.unwrap();

            if let Some(remote) = remote_config.get(remote_name).cloned() {
                log::debug!("fetching remote '{}'", remote.id);
                handles.spawn(Self::fetch_single_remote(
                    remote,
                    status,
                    self.metric_data_tx.clone(),
                    permit,
                ));
            }
        }

        while let Some(res) = handles.join_next().await {
            match res {
                Ok((name, status)) => {
                    self.state.set_status(name, status);
                }
                Err(err) => {
                    log::error!("join error for metric collection task for remote: {err}")
                }
            }
        }
    }

    /// Fetch metric data from remotes which are overdue for collection.
    ///
    /// Use this on startup of the metric collection loop as well as
    /// when the collection interval changes.
    async fn fetch_overdue(
        &mut self,
        remote_config: &SectionConfigData<Remote>,
        next_run: Instant,
        collection_interval: u64,
    ) {
        let left_until_scheduled = next_run - Instant::now();
        let now = proxmox_time::epoch_i64();

        let mut overdue = Vec::new();

        for (remote, _) in remote_config.iter() {
            let last_collection = self
                .state
                .get_status(remote)
                .and_then(|s| s.last_collection)
                .unwrap_or(0);

            let diff = now - last_collection;

            if diff + left_until_scheduled.as_secs() as i64 > collection_interval as i64 {
                log::debug!(
                    "starting metric collection for remote '{remote}' - triggered because collection is overdue"
                );
                overdue.push(remote.into());
            }
        }
        self.fetch_remotes(remote_config, &overdue).await;
    }

    /// Fetch a single remote.
    #[tracing::instrument(skip_all, fields(remote = remote.id), name = "metric_collection_task")]
    async fn fetch_single_remote(
        remote: Remote,
        mut status: RemoteStatus,
        sender: Sender<RrdStoreRequest>,
        _permit: OwnedSemaphorePermit,
    ) -> (String, RemoteStatus) {
        let (result_tx, result_rx) = oneshot::channel();

        let now = proxmox_time::epoch_i64();
        let start = Instant::now();

        let res: Result<RrdStoreResult, Error> = async {
            match remote.ty {
                RemoteType::Pve => {
                    let client = connection::make_pve_client(&remote)?;
                    let metrics = client
                        .cluster_metrics_export(
                            Some(true),
                            Some(false),
                            Some(status.most_recent_datapoint),
                        )
                        .await?;

                    let duration = start.elapsed();

                    sender
                        .send(RrdStoreRequest::Pve {
                            remote: remote.id.clone(),
                            metrics,
                            channel: result_tx,
                            // TODO: use as_millis_f64 once stabilized
                            response_time: duration.as_secs_f64() * 1000.,
                            request_at: now,
                        })
                        .await?;
                }
                RemoteType::Pbs => {
                    let client = connection::make_pbs_client(&remote)?;
                    let metrics = client
                        .metrics(Some(true), Some(status.most_recent_datapoint))
                        .await?;

                    let duration = start.elapsed();

                    sender
                        .send(RrdStoreRequest::Pbs {
                            remote: remote.id.clone(),
                            metrics,
                            channel: result_tx,
                            // TODO: use as_millis_f64 once stabilized
                            response_time: duration.as_secs_f64() * 1000.,
                            request_at: now,
                        })
                        .await?;
                }
            };

            result_rx.await.map_err(Error::from)
        }
        .await;

        match res {
            Ok(result) => {
                status.most_recent_datapoint = result.most_recent_timestamp;
                status.last_collection = Some(now);
                status.error = None;
            }
            Err(err) => {
                status.error = Some(err.to_string());
                log::error!("coud not fetch metrics from '{}': {err}", remote.id);
            }
        }

        (remote.id, status)
    }
}

/// Load the metric collection state file.
pub(super) fn load_state() -> Result<MetricCollectionState, Error> {
    let api_uid = pdm_config::api_user()?.uid;
    let api_gid = pdm_config::api_group()?.gid;

    let file_options = CreateOptions::new().owner(api_uid).group(api_gid);

    Ok(MetricCollectionState::new(
        METRIC_COLLECTION_STATE_FILE.into(),
        file_options,
    ))
}

#[cfg(test)]
pub(super) mod tests {
    use std::sync::Once;

    use anyhow::bail;
    use http::StatusCode;

    use pdm_api_types::Authid;
    use pve_api_types::{ClusterMetrics, ClusterMetricsData};

    use crate::{
        connection::{ClientFactory, PveClient},
        metric_collection::rrd_task::RrdStoreResult,
        pbs_client::PbsClient,
        test_support::temp::NamedTempFile,
    };

    use super::*;

    pub(crate) fn get_create_options() -> CreateOptions {
        CreateOptions::new()
            .owner(nix::unistd::Uid::effective())
            .group(nix::unistd::Gid::effective())
            .perm(nix::sys::stat::Mode::from_bits_truncate(0o600))
    }

    struct TestClientFactory {
        now: i64,
    }

    #[async_trait::async_trait]
    impl ClientFactory for TestClientFactory {
        fn make_pve_client(&self, remote: &Remote) -> Result<Arc<PveClient>, Error> {
            Ok(Arc::new(TestPveClient {
                fail: remote.id.contains("fail"),
                now: self.now,
            }))
        }
        /// Create a new API client for PVE remotes, but with a specific endpoint.
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

        async fn make_pve_client_and_login(
            &self,
            _remote: &Remote,
        ) -> Result<Arc<PveClient>, Error> {
            bail!("not implemented")
        }

        async fn make_pbs_client_and_login(
            &self,
            _remote: &Remote,
        ) -> Result<Box<PbsClient>, Error> {
            bail!("not implemented")
        }
    }

    struct TestPveClient {
        now: i64,
        fail: bool,
    }

    #[async_trait::async_trait]
    impl pve_api_types::client::PveClient for TestPveClient {
        /// Retrieve metrics of the cluster.
        async fn cluster_metrics_export(
            &self,
            _history: Option<bool>,
            _local_only: Option<bool>,
            start_time: Option<i64>,
        ) -> Result<ClusterMetrics, proxmox_client::Error> {
            if self.fail {
                return Err(proxmox_client::Error::Api(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".into(),
                ));
            }

            let mut time = start_time.unwrap_or(0);
            time = time.max(self.now - (30 * 60));
            let mut data = Vec::new();

            use pve_api_types::ClusterMetricsDataType::*;

            while time < self.now {
                let point = |id: &str, metric: &str, timestamp, ty| ClusterMetricsData {
                    id: id.into(),
                    metric: metric.into(),
                    timestamp,
                    ty,
                    value: 0.1,
                };

                for i in 0..5 {
                    let id = format!("node/node-{i}");
                    data.push(point(&id, "cpu_current", time, Gauge));
                }

                // Advance time by 10 seconds
                time += 10;
            }

            Ok(ClusterMetrics { data })
        }
    }

    fn make_remote_config() -> SectionConfigData<Remote> {
        let mut sections = SectionConfigData::default();

        for i in 0..4 {
            let status = if i >= 2 { "fail" } else { "pass" };
            let name = format!("pve-{i}-{status}");

            sections.insert(
                name.clone(),
                Remote {
                    ty: pdm_api_types::remotes::RemoteType::Pve,
                    id: name.clone(),
                    nodes: Vec::new(),
                    authid: Authid::root_auth_id().clone(),
                    token: "".into(),
                    web_url: None,
                },
            );
        }

        sections
    }

    async fn fake_rrd_task(mut rx: Receiver<RrdStoreRequest>) -> u32 {
        let mut number_of_requests = 0;

        while let Some(request) = rx.recv().await {
            number_of_requests += 1;

            if let RrdStoreRequest::Pve {
                metrics, channel, ..
            } = request
            {
                let most_recent_timestamp =
                    metrics.data.iter().map(|e| e.timestamp).max().unwrap_or(0);

                let _ = channel.send(RrdStoreResult {
                    most_recent_timestamp,
                });
            }
        }

        number_of_requests
    }

    static START: Once = Once::new();

    fn test_init() -> i64 {
        let now = 10000;
        START.call_once(|| {
            // TODO: the client factory is currently stored in a OnceLock -
            // we can only set it from one test... Ideally we'd like to have the
            // option to set it in every single test if needed - task/thread local?
            connection::init(Box::new(TestClientFactory { now }));
        });

        now
    }

    #[tokio::test]
    async fn test_fetch_remotes_updates_state() {
        // Arrange
        let now = test_init();

        let (tx, rx) = tokio::sync::mpsc::channel(10);
        let handle = tokio::task::spawn(fake_rrd_task(rx));

        let config = make_remote_config();

        let state_file = NamedTempFile::new(get_create_options()).unwrap();
        let state = MetricCollectionState::new(state_file.path().into(), get_create_options());

        let (_control_tx, control_rx) = tokio::sync::mpsc::channel(10);

        let mut task = MetricCollectionTask {
            state,
            metric_data_tx: tx,
            control_message_rx: control_rx,
        };

        // Act
        let to_fetch = config
            .iter()
            .map(|(name, _)| name.into())
            .collect::<Vec<String>>();
        task.fetch_remotes(&config, &to_fetch).await;

        // Assert
        for remote in &to_fetch {
            let status = task.state.get_status(remote).unwrap();

            // Our faked PVE client will return an error if the remote name contains
            // 'fail'.
            if remote.contains("fail") {
                assert!(status
                    .error
                    .as_ref()
                    .unwrap()
                    .contains("internal server error"));
                assert_eq!(status.last_collection, None);
            } else {
                assert!(now - status.most_recent_datapoint <= 10);
                assert!(status.error.is_none());
            }
        }

        drop(task);
        assert_eq!(handle.await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_fetch_overdue() {
        // Arrange
        test_init();

        let (tx, rx) = tokio::sync::mpsc::channel(10);
        let handle = tokio::task::spawn(fake_rrd_task(rx));

        let config = make_remote_config();

        let state_file = NamedTempFile::new(get_create_options()).unwrap();
        let mut state = MetricCollectionState::new(state_file.path().into(), get_create_options());

        let now = proxmox_time::epoch_i64();

        // This one should be fetched
        state.set_status(
            "pve-0-pass".into(),
            RemoteStatus {
                last_collection: Some(now - 35),
                ..Default::default()
            },
        );
        // This one should *not* be fetched
        state.set_status(
            "pve-1-pass".into(),
            RemoteStatus {
                last_collection: Some(now - 25),
                ..Default::default()
            },
        );

        let (_control_tx, control_rx) = tokio::sync::mpsc::channel(10);

        let mut task = MetricCollectionTask {
            state,
            metric_data_tx: tx,
            control_message_rx: control_rx,
        };

        let next_collection = Instant::now() + Duration::from_secs(30);

        // Act
        task.fetch_overdue(&config, next_collection, 60).await;

        // Assert
        let status = task.state.get_status("pve-0-pass").unwrap();
        assert!(status.last_collection.unwrap() - now >= 0);

        let status = task.state.get_status("pve-1-pass").unwrap();
        assert_eq!(status.last_collection.unwrap(), now - 25);

        drop(task);
        assert_eq!(handle.await.unwrap(), 1);
    }
}
