//! Periodic Ceph cluster auto-detection.
//!
//! Probes every PVE remote for a configured Ceph cluster on a fixed interval
//! and upserts `ceph-clusters.cfg` (see [`server::ceph::sweep`]). This catches
//! clusters on remotes that were added while no detection ran (e.g. before this
//! daemon version), and refreshes the cached `ceph status` as a side effect so
//! the cluster-list overview keeps showing health without a live fetch.

use std::future::Future;
use std::pin::pin;

use tokio::task::JoinHandle;

use server::task_utils;

/// How often to sweep all PVE remotes for Ceph clusters, in seconds.
const SWEEP_INTERVAL: u64 = 300;

fn spawn_aborted_on_shutdown<F>(future: F) -> JoinHandle<()>
where
    F: Future + Send + 'static,
{
    tokio::spawn(async move {
        let future = pin!(future);
        let abort_future = pin!(proxmox_daemon::shutdown_future());
        futures::future::select(future, abort_future).await;
    })
}

pub fn start_task() {
    spawn_aborted_on_shutdown(run());
}

async fn run() {
    loop {
        run_once().await;
        let delay_target = task_utils::next_aligned_instant(SWEEP_INTERVAL);
        tokio::time::sleep_until(tokio::time::Instant::from_std(delay_target)).await;
    }
}

#[tracing::instrument(skip_all, name = "ceph_detection")]
async fn run_once() {
    if let Err(err) = server::ceph::sweep::sweep().await {
        log::warn!("ceph auto-detection sweep failed: {err:#}");
    }
}
