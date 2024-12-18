use std::pin::pin;

use anyhow::Error;

use crate::task_utils;

const COLLECTION_INTERVAL: u64 = 60 * 60; // once per hour

/// Start the resource caching.
pub fn start_task() {
    tokio::spawn(async move {
        let task_scheduler = pin!(resource_caching_task());
        let abort_future = pin!(proxmox_daemon::shutdown_future());
        futures::future::select(task_scheduler, abort_future).await;
    });
}

// FIXME: handle many remotes more intelligently?
async fn resource_caching_task() -> Result<(), Error> {
    loop {
        if let Err(err) = crate::api::resources::get_resources(COLLECTION_INTERVAL, None).await {
            log::error!("could not update resource cache: {err}");
        }

        let delay_target = task_utils::next_aligned_instant(COLLECTION_INTERVAL + 10);
        tokio::time::sleep_until(tokio::time::Instant::from_std(delay_target)).await;
    }
}
