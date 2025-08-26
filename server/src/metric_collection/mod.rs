use std::collections::HashMap;
use std::pin::pin;

use anyhow::Error;
use tokio::sync::mpsc::{self, Sender};

use pdm_api_types::remotes::RemoteType;

use crate::{connection, task_utils};

pub mod rrd_cache;
mod rrd_task;
pub mod top_entities;

use rrd_task::RrdStoreRequest;

const COLLECTION_INTERVAL: u64 = 60;

/// Initialize the RRD cache
pub fn init() -> Result<(), Error> {
    rrd_cache::init()?;

    Ok(())
}

/// Start the metric collection task.
pub fn start_task() {
    let (tx, rx) = mpsc::channel(128);

    tokio::spawn(async move {
        let task_scheduler = pin!(metric_collection_task(tx));
        let abort_future = pin!(proxmox_daemon::shutdown_future());
        futures::future::select(task_scheduler, abort_future).await;
    });

    tokio::spawn(async move {
        let task_scheduler = pin!(rrd_task::store_in_rrd_task(rx));
        let abort_future = pin!(proxmox_daemon::shutdown_future());
        futures::future::select(task_scheduler, abort_future).await;
    });
}

async fn metric_collection_task(sender: Sender<RrdStoreRequest>) -> Result<(), Error> {
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

        for (remote_name, remote) in remotes {
            let start_time = *most_recent_timestamps.get(&remote_name).unwrap_or(&0);
            let remote_name_clone = remote_name.clone();

            let res = async {
                let most_recent_timestamp = match remote.ty {
                    RemoteType::Pve => {
                        let client = connection::make_pve_client(&remote)?;
                        let metrics = client
                            .cluster_metrics_export(Some(true), Some(false), Some(start_time))
                            .await?;

                        let most_recent =
                            metrics.data.iter().fold(0, |acc, x| acc.max(x.timestamp));

                        sender
                            .send(RrdStoreRequest::Pve {
                                remote: remote_name_clone,
                                metrics,
                            })
                            .await?;

                        Ok::<i64, Error>(most_recent)
                    }
                    RemoteType::Pbs => {
                        let client = connection::make_pbs_client(&remote)?;
                        let metrics = client.metrics(Some(true), Some(start_time)).await?;

                        let most_recent =
                            metrics.data.iter().fold(0, |acc, x| acc.max(x.timestamp));

                        sender
                            .send(RrdStoreRequest::Pbs {
                                remote: remote_name_clone,
                                metrics,
                            })
                            .await?;

                        Ok::<i64, Error>(most_recent)
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
