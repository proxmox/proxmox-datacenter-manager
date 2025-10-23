use anyhow::Error;

use server::{remote_updates, task_utils};

const REFRESH_TIME: u64 = 6 * 3600;

/// Start the remote task fetching task
pub fn start_task() -> Result<(), Error> {
    tokio::spawn(async move {
        let task_scheduler = std::pin::pin!(RemoteUpdateRefreshTask {}.run());
        let abort_future = std::pin::pin!(proxmox_daemon::shutdown_future());
        futures::future::select(task_scheduler, abort_future).await;
    });

    Ok(())
}

struct RemoteUpdateRefreshTask {}

impl RemoteUpdateRefreshTask {
    async fn run(self) {
        loop {
            self.refresh().await;
            self.wait_for_refresh().await;
        }
    }

    async fn refresh(&self) {
        if let Err(err) = self.do_refresh().await {
            log::error!("could not refresh remote update cache: {err:#}");
        }
    }

    async fn do_refresh(&self) -> Result<(), Error> {
        let (config, _digest) = tokio::task::spawn_blocking(pdm_config::remotes::config).await??;
        remote_updates::refresh_update_summary_cache(config.into_iter().map(|(_, r)| r).collect())
            .await
    }

    async fn wait_for_refresh(&self) {
        let instant = task_utils::next_aligned_instant(REFRESH_TIME);
        tokio::time::sleep_until(instant.into()).await;
    }
}
