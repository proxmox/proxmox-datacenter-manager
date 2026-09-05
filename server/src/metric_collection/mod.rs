use std::pin::pin;
use std::sync::Arc;
use std::sync::OnceLock;

use anyhow::{Error, bail, format_err};
use nix::sys::stat::Mode;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{self, Sender};
use tokio::sync::oneshot;

use pdm_api_types::RemoteMetricCollectionStatus;
use pdm_buildcfg::PDM_STATE_DIR_M;

mod local_collection_task;
mod remote_collection_task;
pub mod rrd_cache;
mod rrd_task;
mod state;
pub mod top_entities;

use remote_collection_task::{ControlMsg, RemoteMetricCollectionTask};
use rrd_cache::RrdCache;

use crate::metric_collection::local_collection_task::LocalMetricCollectionTask;

const RRD_CACHE_BASEDIR: &str = concat!(PDM_STATE_DIR_M!(), "/rrdb");

static CONTROL_MESSAGE_TX: OnceLock<Sender<ControlMsg>> = OnceLock::new();

/// Initialize the RRD cache
pub fn init() -> Result<(), Error> {
    let file_options = proxmox_product_config::default_create_options();
    let mode = Mode::from_bits_truncate(0o0750);
    let dir_options = file_options.perm(mode);

    let cache = RrdCache::new(RRD_CACHE_BASEDIR, dir_options, file_options)?;
    rrd_cache::set_cache(Arc::new(cache))?;

    Ok(())
}

/// Start the metric collection task.
pub fn start_task() -> Result<(), Error> {
    let (metric_data_tx, metric_data_rx) = mpsc::channel(128);

    let cache = rrd_cache::get_cache();
    tokio::spawn(async move {
        let rrd_task_future = pin!(rrd_task::store_in_rrd_task(cache, metric_data_rx));
        let abort_future = pin!(proxmox_daemon::shutdown_future());
        futures::future::select(rrd_task_future, abort_future).await;
    });

    let (trigger_collection_tx, trigger_collection_rx) = mpsc::channel(128);
    if CONTROL_MESSAGE_TX.set(trigger_collection_tx).is_err() {
        bail!("control message sender already set");
    }

    let metric_data_tx_clone = metric_data_tx.clone();
    tokio::spawn(async move {
        let metric_collection_task_future = pin!(async move {
            match RemoteMetricCollectionTask::new(metric_data_tx_clone, trigger_collection_rx) {
                Ok(mut task) => task.run().await,
                Err(err) => log::error!("could not start metric collection task: {err}"),
            }
        });

        let abort_future = pin!(proxmox_daemon::shutdown_future());
        futures::future::select(metric_collection_task_future, abort_future).await;
    });

    tokio::spawn(async move {
        let metric_collection_task_future =
            pin!(async move { LocalMetricCollectionTask::new(metric_data_tx).run().await });

        let abort_future = pin!(proxmox_daemon::shutdown_future());
        futures::future::select(metric_collection_task_future, abort_future).await;
    });

    Ok(())
}

/// Schedule metric collection as soon as possible.
///
/// If `remote` is `Some(String)`, then the remote with the given ID is
/// collected. If remote is `None`, all remotes are scheduled for collection.
/// If `wait` is `true`, this function waits for the completion of the requested
/// metric collection run.
///
/// Has no effect if the tx end of the channel has not been initialized yet.
/// Returns an error if the mpsc channel has been closed already, or if it is
/// full and `wait` is `false`.
pub async fn trigger_remote_metric_collection(
    remote: Option<String>,
    wait: bool,
) -> Result<(), Error> {
    if let Some(sender) = CONTROL_MESSAGE_TX.get() {
        send_trigger(sender, remote, wait).await?;
    }

    Ok(())
}

async fn send_trigger(
    sender: &Sender<ControlMsg>,
    remote: Option<String>,
    wait: bool,
) -> Result<(), Error> {
    let (done_sender, done_receiver) = oneshot::channel();
    let message = ControlMsg::TriggerMetricCollection(remote, done_sender);

    if wait {
        sender.send(message).await?;
        done_receiver.await?;
    } else {
        // A best-effort trigger must not stall its caller behind a full control queue. The next
        // periodic collection run covers the remote anyway.
        sender.try_send(message).map_err(|err| match err {
            TrySendError::Full(_) => {
                format_err!("too many pending metric collection requests, try again later")
            }
            closed => closed.into(),
        })?;
    }

    Ok(())
}

/// Get each remote's metric collection status.
pub fn remote_metric_collection_status() -> Result<Vec<RemoteMetricCollectionStatus>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let state = remote_collection_task::load_state()?;

    let mut result = Vec::new();

    for (remote, _) in remotes.into_iter() {
        if let Some(status) = state.get_status(&remote) {
            result.push(RemoteMetricCollectionStatus {
                remote,
                error: status.error.clone(),
                last_collection: status.last_collection,
            })
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn best_effort_trigger_does_not_wait_for_queue_capacity() {
        let (sender, _receiver) = mpsc::channel(1);
        let (done_sender, _done_receiver) = oneshot::channel();
        sender
            .try_send(ControlMsg::TriggerMetricCollection(None, done_sender))
            .unwrap();

        let result = tokio::time::timeout(
            Duration::from_millis(100),
            send_trigger(&sender, Some("pve-a".to_string()), false),
        )
        .await
        .expect("best-effort trigger waited for a full control queue");

        let error = result.expect_err("a full control queue must reject the trigger");
        assert!(error.to_string().contains("too many pending"));
    }
}
