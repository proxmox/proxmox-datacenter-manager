use std::sync::Arc;

use anyhow::Error;
use tokio::sync::{mpsc::Receiver, oneshot};

use proxmox_rrd::rrd::DataSourceType;

use pbs_api_types::{MetricDataPoint, MetricDataType, Metrics};
use pve_api_types::{ClusterMetrics, ClusterMetricsData, ClusterMetricsDataType};

use super::local_collection_task::PdmHostMetrics;
use super::rrd_cache::RrdCache;

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
        /// Response time in ms for the API request.
        response_time: f64,
        /// Timestamp at which the request was done (UNIX epoch).
        request_at: i64,
    },
    /// Store PBS metrics.
    Pbs {
        /// Remote name.
        remote: String,
        /// Metric data.
        metrics: Metrics,
        /// Oneshot channel to return the [`RrdStoreResult`].
        channel: oneshot::Sender<RrdStoreResult>,
        /// Response time in ms for the API request.
        response_time: f64,
        /// Timestamp at which the request was done (UNIX epoch).
        request_at: i64,
    },
    /// Store collection stats.
    CollectionStats {
        /// Timestamp at which the collection took place (UNIX epoch).
        timestamp: i64,
        /// Statistics.
        stats: CollectionStats,
    },
    /// Store PDM host metrics.
    Host {
        /// Timestamp at which the metrics were collected (UNIX epoch).
        timestamp: i64,

        /// Metric data for this PDM host.
        // Boxed to avoid a clippy warning regarding large size differences between
        // enum variants.
        metrics: Box<PdmHostMetrics>,
    },
}

/// Result for a [`RrdStoreRequest`].
pub(super) struct RrdStoreResult {
    /// Most recent timestamp of any stored metric datapoint (UNIX epoch).
    pub(super) most_recent_timestamp: i64,
}

/// Statistics for a (full) metric collection run.
pub(super) struct CollectionStats {
    /// Total time in ms.
    pub(super) total_time: f64,
}

/// Datapoints dated more than this far past the request time are dropped: a future-skewed sample
/// would otherwise poison the RRD and silently drop later real samples.
const FUTURE_DATAPOINT_TOLERANCE: i64 = 60;

/// Store a remote's metric datapoints and return the most recent timestamp seen.
///
/// Datapoints dated too far into the future (see [`FUTURE_DATAPOINT_TOLERANCE`]) are dropped.
fn store_datapoints<T>(
    cache: &RrdCache,
    remote: &str,
    data: Vec<T>,
    request_at: i64,
    timestamp_of: impl Fn(&T) -> i64,
    store: impl Fn(&RrdCache, &str, &T),
) -> i64 {
    let future_cutoff = request_at + FUTURE_DATAPOINT_TOLERANCE;
    let mut most_recent_timestamp = 0;
    let mut skipped_future = 0u64;
    for data_point in data {
        let timestamp = timestamp_of(&data_point);
        if timestamp > future_cutoff {
            skipped_future += 1;
            continue;
        }
        most_recent_timestamp = most_recent_timestamp.max(timestamp);
        store(cache, remote, &data_point);
    }
    if skipped_future > 0 {
        log::warn!(
            "ignored {skipped_future} future-dated metric datapoint(s) \
             from {remote:?} (clock skew?)"
        );
    }
    most_recent_timestamp
}

/// Task which stores received metrics in the RRD. Metric data is fed into
/// this task via a MPSC channel.
pub(super) async fn store_in_rrd_task(
    cache: Arc<RrdCache>,
    mut receiver: Receiver<RrdStoreRequest>,
) -> Result<(), Error> {
    while let Some(msg) = receiver.recv().await {
        let cache_clone = Arc::clone(&cache);
        // Involves some blocking file IO
        let res = tokio::task::spawn_blocking(move || {
            match msg {
                RrdStoreRequest::Pve {
                    remote,
                    metrics,
                    channel,
                    response_time,
                    request_at,
                } => {
                    let most_recent_timestamp = store_datapoints(
                        &cache_clone,
                        &remote,
                        metrics.data,
                        request_at,
                        |data_point| data_point.timestamp,
                        store_metric_pve,
                    );
                    store_response_time(&cache_clone, &remote, response_time, request_at);

                    if channel
                        .send(RrdStoreResult {
                            most_recent_timestamp,
                        })
                        .is_err()
                    {
                        log::error!("could not send RrdStoreStoreResult to metric collection task");
                    }
                }
                RrdStoreRequest::Pbs {
                    remote,
                    metrics,
                    channel,
                    response_time,
                    request_at,
                } => {
                    let most_recent_timestamp = store_datapoints(
                        &cache_clone,
                        &remote,
                        metrics.data,
                        request_at,
                        |data_point| data_point.timestamp,
                        store_metric_pbs,
                    );
                    store_response_time(&cache_clone, &remote, response_time, request_at);

                    if channel
                        .send(RrdStoreResult {
                            most_recent_timestamp,
                        })
                        .is_err()
                    {
                        log::error!("could not send RrdStoreStoreResult to metric collection task");
                    }
                }
                RrdStoreRequest::CollectionStats { timestamp, stats } => {
                    store_stats(&cache_clone, &stats, timestamp)
                }
                RrdStoreRequest::Host { timestamp, metrics } => {
                    store_pdm_host_metrics(&cache_clone, timestamp, &metrics)
                }
            };
        })
        .await;

        if let Err(err) = res {
            log::error!("error in rrd task when attempting to save metrics: {err}");
        }
    }

    Ok(())
}

fn store_metric_pve(cache: &RrdCache, remote_name: &str, data_point: &ClusterMetricsData) {
    let name = format!(
        "pve/{remote_name}/{id}/{metric}",
        id = data_point.id,
        metric = data_point.metric,
    );

    let data_source_type = match data_point.ty {
        ClusterMetricsDataType::Gauge => DataSourceType::Gauge,
        ClusterMetricsDataType::Counter => DataSourceType::Counter,
        ClusterMetricsDataType::Derive => DataSourceType::Derive,
        ClusterMetricsDataType::UnknownEnumValue(value) => {
            log::warn!("encountered unknown metric type: {value}");
            return;
        }
    };

    cache.update_value(
        &name,
        data_point.value,
        data_point.timestamp,
        data_source_type,
    );
}

fn store_metric_pbs(cache: &RrdCache, remote_name: &str, data_point: &MetricDataPoint) {
    let name = format!(
        "pbs/{remote_name}/{id}/{metric}",
        id = data_point.id,
        metric = data_point.metric,
    );

    let data_source_type = match data_point.ty {
        MetricDataType::Gauge => DataSourceType::Gauge,
        MetricDataType::Counter => DataSourceType::Counter,
        MetricDataType::Derive => DataSourceType::Derive,
        MetricDataType::UnknownEnumValue(s) => {
            log::warn!("unknown metric data type: {s}");
            return;
        }
    };

    cache.update_value(
        &name,
        data_point.value,
        data_point.timestamp,
        data_source_type,
    );
}

fn store_response_time(cache: &RrdCache, remote_name: &str, response_time: f64, timestamp: i64) {
    let name = format!("remotes/{remote_name}/metric-collection-response-time");

    cache.update_value(&name, response_time, timestamp, DataSourceType::Gauge);
}

fn store_stats(cache: &RrdCache, stats: &CollectionStats, timestamp: i64) {
    cache.update_value(
        "nodes/localhost/metric-collection-total-time",
        stats.total_time,
        timestamp,
        DataSourceType::Gauge,
    );
}

fn store_pdm_host_metrics(cache: &RrdCache, timestamp: i64, metrics: &PdmHostMetrics) {
    if let Some(proc) = &metrics.proc {
        cache.update_value(
            "nodes/localhost/cpu-current",
            proc.cpu,
            timestamp,
            DataSourceType::Gauge,
        );
        cache.update_value(
            "nodes/localhost/cpu-iowait",
            proc.iowait_percent,
            timestamp,
            DataSourceType::Gauge,
        );
    }

    if let Some(load) = &metrics.load {
        cache.update_value(
            "nodes/localhost/cpu-avg1",
            load.0,
            timestamp,
            DataSourceType::Gauge,
        );
        cache.update_value(
            "nodes/localhost/cpu-avg5",
            load.1,
            timestamp,
            DataSourceType::Gauge,
        );
        cache.update_value(
            "nodes/localhost/cpu-avg15",
            load.2,
            timestamp,
            DataSourceType::Gauge,
        );
    }

    if let Some(cpu_pressure) = &metrics.cpu_pressure {
        cache.update_value(
            "nodes/localhost/cpu-pressure-some-avg10",
            cpu_pressure.some.average_10,
            timestamp,
            DataSourceType::Gauge,
        );

        // NOTE: On a system level, 'full' CPU pressure is undefined and reported as 0,
        // so it does not make sense to store it.
        // https://docs.kernel.org/accounting/psi.html#pressure-interface
    }

    if let Some(meminfo) = &metrics.meminfo {
        cache.update_value(
            "nodes/localhost/mem-total",
            meminfo.memtotal as f64,
            timestamp,
            DataSourceType::Gauge,
        );
        cache.update_value(
            "nodes/localhost/mem-used",
            meminfo.memused as f64,
            timestamp,
            DataSourceType::Gauge,
        );
        cache.update_value(
            "nodes/localhost/swap-total",
            meminfo.swaptotal as f64,
            timestamp,
            DataSourceType::Gauge,
        );
        cache.update_value(
            "nodes/localhost/swap-used",
            meminfo.swapused as f64,
            timestamp,
            DataSourceType::Gauge,
        );
    }

    if let Some(memory_pressure) = &metrics.memory_pressure {
        cache.update_value(
            "nodes/localhost/mem-pressure-some-avg10",
            memory_pressure.some.average_10,
            timestamp,
            DataSourceType::Gauge,
        );
        cache.update_value(
            "nodes/localhost/mem-pressure-full-avg10",
            memory_pressure.full.average_10,
            timestamp,
            DataSourceType::Gauge,
        );
    }

    if let Some(netstats) = &metrics.netstats {
        cache.update_value(
            "nodes/localhost/net-in",
            netstats.netin as f64,
            timestamp,
            DataSourceType::Derive,
        );
        cache.update_value(
            "nodes/localhost/net-out",
            netstats.netout as f64,
            timestamp,
            DataSourceType::Derive,
        );
    }

    if let Some(disk) = &metrics.root_filesystem_info {
        cache.update_value(
            "nodes/localhost/disk-total",
            disk.total as f64,
            timestamp,
            DataSourceType::Gauge,
        );
        cache.update_value(
            "nodes/localhost/disk-used",
            disk.used as f64,
            timestamp,
            DataSourceType::Gauge,
        );
    }

    if let Some(stat) = &metrics.root_blockdev_stat {
        cache.update_value(
            "nodes/localhost/disk-read-iops",
            stat.read_ios as f64,
            timestamp,
            DataSourceType::Derive,
        );
        cache.update_value(
            "nodes/localhost/disk-write-iops",
            stat.write_ios as f64,
            timestamp,
            DataSourceType::Derive,
        );
        cache.update_value(
            "nodes/localhost/disk-read",
            (stat.read_sectors * 512) as f64,
            timestamp,
            DataSourceType::Derive,
        );
        cache.update_value(
            "nodes/localhost/disk-write",
            (stat.write_sectors * 512) as f64,
            timestamp,
            DataSourceType::Derive,
        );
        cache.update_value(
            "nodes/localhost/disk-io-ticks",
            (stat.io_ticks as f64) / 1000.0,
            timestamp,
            DataSourceType::Derive,
        );
    }

    if let Some(io_pressure) = &metrics.io_pressure {
        cache.update_value(
            "nodes/localhost/io-pressure-some-avg10",
            io_pressure.some.average_10,
            timestamp,
            DataSourceType::Gauge,
        );
        cache.update_value(
            "nodes/localhost/io-pressure-full-avg10",
            io_pressure.full.average_10,
            timestamp,
            DataSourceType::Gauge,
        );
    }
}

#[cfg(test)]
mod tests {
    use proxmox_rrd_api_types::{RrdMode, RrdTimeframe};
    use pve_api_types::{ClusterMetrics, ClusterMetricsData};

    use crate::{
        metric_collection::remote_collection_task::tests::get_create_options,
        test_support::temp::NamedTempDir,
    };

    use super::*;

    #[tokio::test]
    async fn test_rrd_task_persists_data() -> Result<(), Error> {
        // Arrange
        let dir = NamedTempDir::new()?;
        let options = get_create_options().perm(nix::sys::stat::Mode::from_bits_truncate(0o700));
        let cache = Arc::new(RrdCache::new(dir.path(), options, options)?);

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
            response_time: 10.0,
            request_at: now,
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
        if let Some(data) = cache.extract_data(
            "pve/some-remote/node/some-node",
            "cpu_current",
            RrdTimeframe::Hour,
            RrdMode::Max,
        )? {
            // Only assert that there are some data points, the exact position in the vec
            // might vary due to changed boundaries.
            assert!(data.data.iter().any(Option::is_some));
        }

        if let Some(data) = cache.extract_data(
            "remotes/some-remote",
            "metric-collection-response-time",
            RrdTimeframe::Hour,
            RrdMode::Max,
        )? {
            assert!(data.data.iter().any(Option::is_some));
        }

        Ok(())
    }
}
