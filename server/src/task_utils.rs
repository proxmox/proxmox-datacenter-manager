use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Returns an `Instant` aligned to a certain boundary.
///
/// For instance, `aligned_instant(60)` will return an `Instant` aligned
/// to the next minute boundary.
pub fn next_aligned_instant(seconds: u64) -> Instant {
    let now = SystemTime::now();
    let epoch_now = match now.duration_since(UNIX_EPOCH) {
        Ok(d) => d,
        Err(err) => {
            log::error!("task scheduler: computing next aligned instant failed - {err}");
            return Instant::now() + Duration::from_secs(seconds);
        }
    };
    Instant::now() + Duration::from_secs(seconds - epoch_now.as_secs().rem_euclid(seconds))
}
