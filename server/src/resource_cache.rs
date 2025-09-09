use std::pin::pin;

use anyhow::Error;

use crate::task_utils;

// This is the interval we update the cache independent of any API / UI activity, but depending on
// the max-age from API calls the caches can get updated more frequently.
const METRIC_POLL_INTERVALL: u64 = 15 * 60; // once every 15 minutes

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
        if let Err(err) =
            crate::api::resources::get_resources_impl(METRIC_POLL_INTERVALL, None, None, None).await
        {
            log::error!("could not update resource cache: {err}");
        }

        let delay_target = task_utils::next_aligned_instant(METRIC_POLL_INTERVALL + 10);
        tokio::time::sleep_until(tokio::time::Instant::from_std(delay_target)).await;
    }
}
