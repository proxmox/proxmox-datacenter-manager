use anyhow::Error;
use tokio::sync::{mpsc::Receiver, oneshot};

use proxmox_rrd::rrd::DataSourceType;

use pbs_api_types::{MetricDataPoint, MetricDataType, Metrics};
use pve_api_types::{ClusterMetrics, ClusterMetricsData, ClusterMetricsDataType};

use super::rrd_cache;

/// Store request for the RRD task.
pub(super) enum RrdStoreRequest {
    /// Store PVE metrics.
    Pve {
        /// Remote name.
        remote: String,
        /// Metric data.
        metrics: ClusterMetrics,
        /// Oneshot channel to return the [`RrdStoreResult`].
        channel: oneshot::Sender<RrdStoreResult>,
    },
    /// Store PBS metrics.
    Pbs {
        /// Remote name.
        remote: String,
        /// Metric data.
        metrics: Metrics,
        /// Oneshot channel to return the [`RrdStoreResult`].
        channel: oneshot::Sender<RrdStoreResult>,
    },
}

/// Result for a [`RrdStoreRequest`].
pub(super) struct RrdStoreResult {
    /// Most recent timestamp of any stored metric datapoint (UNIX epoch).
    pub(super) most_recent_timestamp: i64,
}

/// Task which stores received metrics in the RRD. Metric data is fed into
/// this task via a MPSC channel.
pub(super) async fn store_in_rrd_task(
    mut receiver: Receiver<RrdStoreRequest>,
) -> Result<(), Error> {
    while let Some(msg) = receiver.recv().await {
        // Involves some blocking file IO
        let res = tokio::task::spawn_blocking(move || {
            let mut most_recent_timestamp = 0;
            let channel = match msg {
                RrdStoreRequest::Pve {
                    remote,
                    metrics,
                    channel,
                } => {
                    for data_point in metrics.data {
                        most_recent_timestamp = most_recent_timestamp.max(data_point.timestamp);
                        store_metric_pve(&remote, &data_point);
                    }

                    channel
                }
                RrdStoreRequest::Pbs {
                    remote,
                    metrics,
                    channel,
                } => {
                    for data_point in metrics.data {
                        most_recent_timestamp = most_recent_timestamp.max(data_point.timestamp);
                        store_metric_pbs(&remote, &data_point);
                    }

                    channel
                }
            };

            if channel
                .send(RrdStoreResult {
                    most_recent_timestamp,
                })
                .is_err()
            {
                log::error!("could not send RrdStoreStoreResult to metric collection task");
            };
        })
        .await;

        if let Err(err) = res {
            log::error!("error in rrd task when attempting to save metrics: {err}");
        }
    }

    Ok(())
}

fn store_metric_pve(remote_name: &str, data_point: &ClusterMetricsData) {
    let name = format!(
        "pve/{remote_name}/{id}/{metric}",
        id = data_point.id,
        metric = data_point.metric,
    );

    let data_source_type = match data_point.ty {
        ClusterMetricsDataType::Gauge => DataSourceType::Gauge,
        ClusterMetricsDataType::Counter => DataSourceType::Counter,
        ClusterMetricsDataType::Derive => DataSourceType::Derive,
    };

    rrd_cache::update_value(
        &name,
        data_point.value,
        data_point.timestamp,
        data_source_type,
    );
}

fn store_metric_pbs(remote_name: &str, data_point: &MetricDataPoint) {
    let name = format!(
        "pbs/{remote_name}/{id}/{metric}",
        id = data_point.id,
        metric = data_point.metric,
    );

    let data_source_type = match data_point.ty {
        MetricDataType::Gauge => DataSourceType::Gauge,
        MetricDataType::Counter => DataSourceType::Counter,
        MetricDataType::Derive => DataSourceType::Derive,
    };

    rrd_cache::update_value(
        &name,
        data_point.value,
        data_point.timestamp,
        data_source_type,
    );
}
