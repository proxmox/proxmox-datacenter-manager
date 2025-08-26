use std::sync::Arc;

use anyhow::Error;
use tokio::sync::{mpsc::Receiver, oneshot};

use proxmox_rrd::{rrd::DataSourceType, Cache};

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
    cache: Arc<Cache>,
    mut receiver: Receiver<RrdStoreRequest>,
) -> Result<(), Error> {
    while let Some(msg) = receiver.recv().await {
        let cache_clone = Arc::clone(&cache);
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
                        store_metric_pve(&cache_clone, &remote, &data_point);
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
                        store_metric_pbs(&cache_clone, &remote, &data_point);
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

fn store_metric_pve(cache: &Cache, remote_name: &str, data_point: &ClusterMetricsData) {
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
        cache,
        &name,
        data_point.value,
        data_point.timestamp,
        data_source_type,
    );
}

fn store_metric_pbs(cache: &Cache, remote_name: &str, data_point: &MetricDataPoint) {
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
        cache,
        &name,
        data_point.value,
        data_point.timestamp,
        data_source_type,
    );
}

#[cfg(test)]
mod tests {
    use proxmox_rrd_api_types::{RrdMode, RrdTimeframe};
    use pve_api_types::{ClusterMetrics, ClusterMetricsData};

    use crate::{
        metric_collection::collection_task::tests::get_create_options,
        test_support::temp::NamedTempDir,
    };

    use super::*;

    #[tokio::test]
    async fn test_rrd_task_persists_data() -> Result<(), Error> {
        // Arrange
        let dir = NamedTempDir::new()?;
        let options = get_create_options().perm(nix::sys::stat::Mode::from_bits_truncate(0o700));
        let cache = rrd_cache::init(&dir.path(), options.clone(), options.clone())?;

        let (tx, rx) = tokio::sync::mpsc::channel(10);
        let task = store_in_rrd_task(Arc::clone(&cache), rx);
        let handle = tokio::task::spawn(task);

        let now = proxmox_time::epoch_i64();

        let metrics = ClusterMetrics {
            data: vec![
                ClusterMetricsData {
                    id: "node/some-node".into(),
                    metric: "cpu_current".into(),
                    timestamp: now - 30,
                    ty: ClusterMetricsDataType::Gauge,
                    value: 0.1,
                },
                ClusterMetricsData {
                    id: "node/some-node".into(),
                    metric: "cpu_current".into(),
                    timestamp: now - 20,
                    ty: ClusterMetricsDataType::Gauge,
                    value: 0.2,
                },
                ClusterMetricsData {
                    id: "node/some-node".into(),
                    metric: "cpu_current".into(),
                    timestamp: now - 10,
                    ty: ClusterMetricsDataType::Gauge,
                    value: 0.1,
                },
                ClusterMetricsData {
                    id: "node/some-node".into(),
                    metric: "cpu_current".into(),
                    timestamp: now,
                    ty: ClusterMetricsDataType::Gauge,
                    value: 0.2,
                },
            ],
        };
        let (tx_back, rx_back) = tokio::sync::oneshot::channel();
        let request = RrdStoreRequest::Pve {
            remote: "some-remote".into(),
            metrics,
            channel: tx_back,
        };

        // Act
        tx.send(request).await?;
        let result = rx_back.await?;

        // Assert
        assert_eq!(result.most_recent_timestamp, now);

        drop(tx);
        handle.await??;

        // There is some race condition in proxmox_rrd, in some rare cases
        // extract_data does not return any data directly after writing.
        if let Some(data) = rrd_cache::extract_data(
            &cache,
            "pve/some-remote/node/some-node",
            "cpu_current",
            RrdTimeframe::Hour,
            RrdMode::Max,
        )? {
            // Only assert that there are some data points, the exact position in the vec
            // might vary due to changed boundaries.
            assert!(data.data.iter().any(Option::is_some));
        }

        Ok(())
    }
}
