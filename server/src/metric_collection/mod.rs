use std::pin::pin;
use std::sync::OnceLock;

use anyhow::{bail, Error};
use tokio::sync::mpsc::{self, Sender};

use proxmox_sys::fs::CreateOptions;

mod collection_task;
pub mod rrd_cache;
mod rrd_task;
mod state;
pub mod top_entities;

use collection_task::{ControlMsg, MetricCollectionTask};

static CONTROL_MESSAGE_TX: OnceLock<Sender<ControlMsg>> = OnceLock::new();

/// Initialize the RRD cache
pub fn init() -> Result<(), Error> {
    let api_uid = pdm_config::api_user()?.uid;
    let api_gid = pdm_config::api_group()?.gid;

    let file_options = CreateOptions::new().owner(api_uid).group(api_gid);
    let dir_options = CreateOptions::new().owner(api_uid).group(api_gid);

    let cache = rrd_cache::init(rrd_cache::RRD_CACHE_BASEDIR, dir_options, file_options)?;
    rrd_cache::set_cache(cache)?;

    Ok(())
}

/// Start the metric collection task.
pub fn start_task() -> Result<(), Error> {
    let (metric_data_tx, metric_data_rx) = mpsc::channel(128);

    let (trigger_collection_tx, trigger_collection_rx) = mpsc::channel(128);
    if CONTROL_MESSAGE_TX.set(trigger_collection_tx).is_err() {
        bail!("control message sender alread set");
    }

    tokio::spawn(async move {
        let metric_collection_task_future = pin!(async move {
            match MetricCollectionTask::new(metric_data_tx, trigger_collection_rx) {
                Ok(mut task) => task.run().await,
                Err(err) => log::error!("could not start metric collection task: {err}"),
            }
        });

        let abort_future = pin!(proxmox_daemon::shutdown_future());
        futures::future::select(metric_collection_task_future, abort_future).await;
    });

    let cache = rrd_cache::get_cache();

    tokio::spawn(async move {
        let rrd_task_future = pin!(rrd_task::store_in_rrd_task(cache, metric_data_rx));
        let abort_future = pin!(proxmox_daemon::shutdown_future());
        futures::future::select(rrd_task_future, abort_future).await;
    });

    Ok(())
}

/// Schedule metric collection for a given remote as soon as possible.
///
/// Has no effect if the tx end of the channel has not been initialized yet.
/// Returns an error if the mpsc channel has been closed already.
pub async fn trigger_metric_collection(remote: Option<String>) -> Result<(), Error> {
    if let Some(sender) = CONTROL_MESSAGE_TX.get() {
        sender
            .send(ControlMsg::TriggerMetricCollection(remote))
            .await?;
    }

    Ok(())
}
