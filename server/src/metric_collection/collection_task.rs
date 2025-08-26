use std::{sync::Arc, time::Duration};

use anyhow::Error;
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        oneshot, OwnedSemaphorePermit, Semaphore,
    },
    time::Interval,
};

use proxmox_section_config::typed::SectionConfigData;
use proxmox_sys::fs::CreateOptions;

use pdm_api_types::remotes::{Remote, RemoteType};

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
    TriggerMetricCollection(Option<String>),
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
        let mut timer = Self::setup_timer(DEFAULT_COLLECTION_INTERVAL);

        log::debug!(
            "metric collection starting up. Collection interval set to {} seconds.",
            DEFAULT_COLLECTION_INTERVAL,
        );

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
            let to_fetch = remotes
                .iter()
                .map(|(name, _)| name.into())
                .collect::<Vec<String>>();
            self.fetch_remotes(&remotes, &to_fetch).await;
        }
    }

    /// Handle a control message for force-triggered collection.
    async fn handle_control_message(&mut self, message: ControlMsg) {
        if let Some(remotes) = Self::load_remote_config() {
            match message {
                ControlMsg::TriggerMetricCollection(Some(remote)) => {
                    log::debug!("starting metric collection for remote '{remote}'- triggered by control message");
                    self.fetch_remotes(&remotes, &[remote]).await;
                }
                ControlMsg::TriggerMetricCollection(None) => {
                    log::debug!("starting metric collection from all remotes - triggered by control message");
                    let to_fetch = remotes
                        .iter()
                        .map(|(name, _)| name.into())
                        .collect::<Vec<String>>();
                    self.fetch_remotes(&remotes, &to_fetch).await;
                }
            }
        }
    }

    /// Set up a [`tokio::time::Interval`] instance with the provided interval.
    /// The timer will be aligned, e.g. an interval of `60` will let the timer
    /// fire at minute boundaries.
    fn setup_timer(interval: u64) -> Interval {
        let mut timer = tokio::time::interval(Duration::from_secs(interval));
        let first_run = task_utils::next_aligned_instant(interval).into();
        timer.reset_at(first_run);

        timer
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
        let mut handles = Vec::new();
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
                let handle = tokio::spawn(Self::fetch_single_remote(
                    remote,
                    status,
                    self.metric_data_tx.clone(),
                    permit,
                ));

                handles.push((remote_name.clone(), handle));
            }
        }

        for (remote_name, handle) in handles {
            let res = handle.await;

            match res {
                Ok(Ok(ts)) => {
                    self.state.set_status(remote_name, ts);
                }
                Ok(Err(err)) => log::error!("failed to collect metrics for {remote_name}: {err}"),
                Err(err) => {
                    log::error!(
                        "join error for metric collection task for remote {remote_name}: {err}"
                    )
                }
            }
        }
    }

    /// Fetch a single remote.
    #[tracing::instrument(skip_all, fields(remote = remote.id), name = "metric_collection_task")]
    async fn fetch_single_remote(
        remote: Remote,
        mut status: RemoteStatus,
        sender: Sender<RrdStoreRequest>,
        _permit: OwnedSemaphorePermit,
    ) -> Result<RemoteStatus, Error> {
        let (result_tx, result_rx) = oneshot::channel();

        let now = proxmox_time::epoch_i64();

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

                    sender
                        .send(RrdStoreRequest::Pve {
                            remote: remote.id.clone(),
                            metrics,
                            channel: result_tx,
                        })
                        .await?;
                }
                RemoteType::Pbs => {
                    let client = connection::make_pbs_client(&remote)?;
                    let metrics = client
                        .metrics(Some(true), Some(status.most_recent_datapoint))
                        .await?;

                    sender
                        .send(RrdStoreRequest::Pbs {
                            remote: remote.id.clone(),
                            metrics,
                            channel: result_tx,
                        })
                        .await?;
                }
            }

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

        Ok(status)
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
