use std::time::Duration;

use anyhow::Error;
use nix::sys::stat::Mode;

use server::remote_tasks::{refresh_task, REMOTE_TASKS_DIR};
use server::task_utils;

/// Tick interval for the remote task fetching task.
/// This is also the rate at which we check on tracked tasks.
const POLL_INTERVAL: Duration = Duration::from_secs(10);

/// Start the remote task fetching task
pub fn start_task() -> Result<(), Error> {
    let dir_options =
        proxmox_product_config::default_create_options().perm(Mode::from_bits_truncate(0o0750));

    proxmox_sys::fs::create_path(REMOTE_TASKS_DIR, None, Some(dir_options))?;

    tokio::spawn(async move {
        let task_scheduler = std::pin::pin!(remote_task_fetching_task());
        let abort_future = std::pin::pin!(proxmox_daemon::shutdown_future());
        futures::future::select(task_scheduler, abort_future).await;
    });

    Ok(())
}

/// Task which handles fetching remote tasks and task archive rotation.
/// This function never returns.
async fn remote_task_fetching_task() -> ! {
    let mut task_state = refresh_task::TaskState::new();

    let mut interval = tokio::time::interval(POLL_INTERVAL);
    interval.reset_at(task_utils::next_aligned_instant(POLL_INTERVAL.as_secs()).into());

    // We don't really care about catching up to missed tick, we just want
    // a steady tick rate.
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    if let Err(err) = refresh_task::init_cache().await {
        log::error!("error when initialized task cache: {err:#}");
    }

    loop {
        interval.tick().await;
        if let Err(err) = refresh_task::handle_timer_tick(&mut task_state).await {
            log::error!("error when fetching remote tasks: {err:#}");
        }
    }
}
