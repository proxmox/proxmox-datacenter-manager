use std::{collections::BTreeMap, time::Duration};

use anyhow::{bail, Error};

use proxmox_rrd_api_types::{RrdMode, RrdTimeframe};

use crate::metric_collection::{self, rrd_cache};

/// Trait common to all RRD-stored metric objects (nodes, datastores, qemu, lxc, etc.)
pub trait DataPoint {
    /// Create a new  data point with a given timestamp
    fn new(time: u64) -> Self;
    /// Returns the names of the underlying (stringly typed) fields in the RRD
    fn fields() -> &'static [&'static str];
    /// Set a member by its field identifier
    fn set_field(&mut self, name: &str, value: f64);
}

pub fn create_datapoints_from_rrd<T: DataPoint>(
    basedir: &str,
    timeframe: RrdTimeframe,
    mode: RrdMode,
) -> Result<Vec<T>, Error> {
    let mut timemap = BTreeMap::new();
    let mut last_resolution = None;

    let cache = rrd_cache::get_cache();

    for name in T::fields() {
        let (start, resolution, data) = match cache.extract_data(basedir, name, timeframe, mode)? {
            Some(data) => data.into(),
            None => continue,
        };

        if let Some(expected_resolution) = last_resolution {
            if resolution != expected_resolution {
                bail!("got unexpected RRD resolution ({resolution} != {expected_resolution})",);
            }
        } else {
            last_resolution = Some(resolution);
        }

        let mut t = start;

        for value in data {
            let entry = timemap.entry(t).or_insert_with(|| T::new(t));
            if let Some(value) = value {
                entry.set_field(name, value);
            }

            t += resolution;
        }
    }

    Ok(timemap.into_values().collect())
}

/// Get RRD datapoints for a given remote/RRD path.
///
/// If `timeframe` is set to [`RrdTimeframe::Hour`], then this function will trigger
/// metric collection for this remote and wait for its completion, up to a timeout of five
/// seconds. If the timeout is exceeded, we simply go ahead and return what is in the database at
/// the moment, which might have a gap for the last couple minutes.
pub async fn get_rrd_datapoints<T: DataPoint + Send + 'static>(
    remote: String,
    basepath: String,
    timeframe: RrdTimeframe,
    mode: RrdMode,
) -> Result<Vec<T>, Error> {
    const WAIT_FOR_NEWEST_METRIC_TIMEOUT: Duration = Duration::from_secs(5);

    if timeframe == RrdTimeframe::Hour {
        // Let's wait for a limited time for the most recent metrics. If the connection to the remote
        // is super slow or if the metric collection tasks currently busy with collecting other
        // metrics, we just return the data we already have, not the newest one.
        let _ = tokio::time::timeout(WAIT_FOR_NEWEST_METRIC_TIMEOUT, async {
            metric_collection::trigger_metric_collection(Some(remote), true).await
        })
        .await;
    }

    tokio::task::spawn_blocking(move || create_datapoints_from_rrd(&basepath, timeframe, mode))
        .await?
}
