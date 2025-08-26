use anyhow::Error;
use pbs_api_types::{MetricDataPoint, MetricDataType, Metrics};
use proxmox_rrd::rrd::DataSourceType;
use pve_api_types::{ClusterMetrics, ClusterMetricsData, ClusterMetricsDataType};
use tokio::sync::mpsc::Receiver;

use super::rrd_cache;

/// Store request for the RRD task.
pub(super) enum RrdStoreRequest {
    /// Store PVE metrics.
    Pve {
        /// Remote name.
        remote: String,
        /// Metric data.
        metrics: ClusterMetrics,
    },
    /// Store PBS metrics.
    Pbs {
        /// Remote name.
        remote: String,
        /// Metric data.
        metrics: Metrics,
    },
}

/// Task which stores received metrics in the RRD. Metric data is fed into
/// this task via a MPSC channel.
pub(super) async fn store_in_rrd_task(
    mut receiver: Receiver<RrdStoreRequest>,
) -> Result<(), Error> {
    while let Some(msg) = receiver.recv().await {
        // Involves some blocking file IO
        let res = tokio::task::spawn_blocking(move || match msg {
            RrdStoreRequest::Pve { remote, metrics } => {
                for data_point in metrics.data {
                    store_metric_pve(&remote, &data_point);
                }
            }
            RrdStoreRequest::Pbs { remote, metrics } => {
                for data_point in metrics.data {
                    store_metric_pbs(&remote, &data_point);
                }
            }
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
