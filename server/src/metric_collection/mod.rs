use std::collections::HashMap;
use std::pin::pin;

use anyhow::Error;

use proxmox_rrd::rrd::DataSourceType;

use pdm_api_types::remotes::RemoteType;
use pve_api_types::{ClusterMetricsData, ClusterMetricsDataType};

use crate::api::pve;
use crate::task_utils;

mod rrd_cache;

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

        let (remotes, _) = pdm_config::remotes::config()?;

        for (remote_name, remote) in &remotes.sections {
            let start_time = *most_recent_timestamps.get(remote_name).unwrap_or(&0);
            let remote_name_clone = remote_name.clone();

            let res = async {
                let most_recent_timestamp = match remote.ty {
                    RemoteType::Pve => {
                        let client = pve::connect_to_remote(&remotes, remote_name)?;
                        let metrics = client
                            .cluster_metrics_export(Some(true), Some(false), Some(start_time))
                            .await;

                        let mut data = metrics?.data;
                        // Sort by timestamp: We want to ensure that we
                        // store older data points first.
                        data.sort_unstable_by(|a, b| a.timestamp.cmp(&b.timestamp));

                        // Involves some blocking file IO
                        tokio::task::spawn_blocking(move || {
                            let mut most_recent_timestamp = 0;

                            for data_point in data {
                                most_recent_timestamp =
                                    most_recent_timestamp.max(data_point.timestamp);
                                store_metric("pve", &remote_name_clone, &data_point);
                            }

                            most_recent_timestamp
                        })
                        .await
                    }
                    RemoteType::Pbs => {
                        // Not implemented yet
                        Ok(0)
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

fn store_metric(prefix: &str, remote_name: &str, data_point: &ClusterMetricsData) {
    let name = format!(
        "{prefix}/{remote_name}/{id}/{metric}",
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
