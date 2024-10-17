use std::collections::HashMap;
use std::pin::pin;

use anyhow::Error;

use pbs_api_types::{MetricDataPoint, MetricDataType};
use proxmox_rrd::rrd::DataSourceType;

use pdm_api_types::remotes::RemoteType;
use pve_api_types::{ClusterMetricsData, ClusterMetricsDataType};

use crate::api::pve;
use crate::{pbs_client, task_utils};

pub mod rrd_cache;

const COLLECTION_INTERVAL: u64 = 60;

/// Initialize the RRD cache
pub fn init() -> Result<(), Error> {
    rrd_cache::init()?;
    rrd_cache::sync_journal();

    Ok(())
}

/// Start the metric collection task.
pub fn start_task() {
    tokio::spawn(async move {
        let task_scheduler = pin!(metric_collection_task());
        let abort_future = pin!(proxmox_daemon::shutdown_future());
        futures::future::select(task_scheduler, abort_future).await;
    });
}

async fn metric_collection_task() -> Result<(), Error> {
    let mut most_recent_timestamps: HashMap<String, i64> = HashMap::new();

    loop {
        let delay_target = task_utils::next_aligned_instant(COLLECTION_INTERVAL);
        tokio::time::sleep_until(tokio::time::Instant::from_std(delay_target)).await;

        let remotes = match pdm_config::remotes::config() {
            Ok((remotes, _)) => remotes,
            Err(e) => {
                log::error!("failed to collect metrics, could not read remotes.cfg: {e}");
                continue;
            }
        };

        for (remote_name, remote) in &remotes.sections {
            let start_time = *most_recent_timestamps.get(remote_name).unwrap_or(&0);
            let remote_name_clone = remote_name.clone();

            let res = async {
                let most_recent_timestamp = match remote.ty {
                    RemoteType::Pve => {
                        let client = pve::connect(&remote)?;
                        let metrics = client
                            .cluster_metrics_export(Some(true), Some(false), Some(start_time))
                            .await?;

                        // Involves some blocking file IO
                        tokio::task::spawn_blocking(move || {
                            let mut most_recent_timestamp = 0;

                            for data_point in metrics.data {
                                most_recent_timestamp =
                                    most_recent_timestamp.max(data_point.timestamp);
                                store_metric_pve(&remote_name_clone, &data_point);
                            }

                            most_recent_timestamp
                        })
                        .await
                    }
                    RemoteType::Pbs => {
                        let client = pbs_client::connect(&remote)?;
                        let metrics = client.metrics(Some(true), Some(start_time)).await?;

                        // Involves some blocking file IO
                        tokio::task::spawn_blocking(move || {
                            let mut most_recent_timestamp = 0;

                            for data_point in metrics.data {
                                most_recent_timestamp =
                                    most_recent_timestamp.max(data_point.timestamp);
                                store_metric_pbs(&remote_name_clone, &data_point);
                            }

                            most_recent_timestamp
                        })
                        .await
                    }
                }?;

                Ok::<i64, Error>(most_recent_timestamp)
            }
            .await;

            match res {
                Ok(ts) => {
                    most_recent_timestamps.insert(remote_name.to_string(), ts);
                }
                Err(err) => log::error!("failed to collect metrics for {remote_name}: {err}"),
            }
        }
    }
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
