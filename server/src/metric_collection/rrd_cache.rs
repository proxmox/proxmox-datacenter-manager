//! Round Robin Database cache
//!
//! RRD files are stored under `/var/lib/proxmox-datacenter-manager/rrdb/`. Only a
//! single process may access and update those files, so we initialize
//! and update RRD data inside `proxmox-datacenter-api`.

use std::path::Path;
use std::sync::Arc;

use anyhow::{format_err, Error};
use once_cell::sync::OnceCell;

use proxmox_rrd::rrd::{AggregationFn, Archive, DataSourceType, Database};
use proxmox_rrd::Cache;
use proxmox_rrd_api_types::{RrdMode, RrdTimeframe};
use proxmox_sys::fs::CreateOptions;

// This is an `Arc` because this makes it easier to do dependency injection
// in test contexts.
//
// For DI in testing, we want to pass in a reference to the Cache
// as a function parameter. In a couple of these functions we
// spawn tokio tasks which need access to the reference, hence the
// reference needs to be 'static. In a test context, we kind of have a
// hard time to come up with a 'static reference, so we just
// wrap the cache in an `Arc` for now, solving the
// lifetime problem via refcounting.
static RRD_CACHE: OnceCell<Arc<RrdCache>> = OnceCell::new();

/// Get the RRD cache instance
pub fn get_cache() -> Arc<RrdCache> {
    RRD_CACHE.get().cloned().expect("rrd cache not initialized")
}

pub fn set_cache(cache: Arc<RrdCache>) -> Result<(), Error> {
    RRD_CACHE
        .set(cache)
        .map_err(|_| format_err!("RRD cache already initialized!"))?;

    Ok(())
}

/// Wrapper for proxmox_rrd::Cache to accomodate helper methods.
pub struct RrdCache {
    cache: Cache,
}

impl RrdCache {
    /// Create a new RrdCache instance
    pub fn new<P: AsRef<Path>>(
        base_path: P,
        dir_options: CreateOptions,
        file_options: CreateOptions,
    ) -> Result<Self, Error> {
        let apply_interval = 30.0 * 60.0; // 30 minutes

        let cache = Cache::new(
            base_path,
            Some(file_options),
            Some(dir_options),
            apply_interval,
            Self::load_callback,
            Self::create_callback,
        )?;

        cache.apply_journal()?;

        Ok(Self { cache })
    }

    fn load_callback(path: &Path, _rel_path: &str) -> Option<Database> {
        match Database::load(path, true) {
            Ok(rrd) => Some(rrd),
            Err(err) => {
                if err.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("overwriting RRD file {path:?}, because of load error: {err}",);
                }
                None
            }
        }
    }

    fn create_callback(dst: DataSourceType) -> Database {
        let rra_list = vec![
            // 1 min * 1440 => 1 day
            Archive::new(AggregationFn::Average, 60, 1440),
            Archive::new(AggregationFn::Maximum, 60, 1440),
            // 30 min * 1440 => 30 days ~ 1 month
            Archive::new(AggregationFn::Average, 30 * 60, 1440),
            Archive::new(AggregationFn::Maximum, 30 * 60, 1440),
            // 6 h * 1440 => 360 days ~ 1 year
            Archive::new(AggregationFn::Average, 6 * 3600, 1440),
            Archive::new(AggregationFn::Maximum, 6 * 3600, 1440),
            // 1 week * 570 => 10 years
            Archive::new(AggregationFn::Average, 7 * 86400, 570),
            Archive::new(AggregationFn::Maximum, 7 * 86400, 570),
        ];

        Database::new(dst, rra_list)
    }

    /// Extracts data for the specified time frame from RRD cache
    pub fn extract_data(
        &self,
        basedir: &str,
        name: &str,
        timeframe: RrdTimeframe,
        mode: RrdMode,
    ) -> Result<Option<proxmox_rrd::Entry>, Error> {
        let end = proxmox_time::epoch_f64() as u64;

        let (start, resolution) = match timeframe {
            RrdTimeframe::Hour => (end - 3600, 60),
            RrdTimeframe::Day => (end - 3600 * 24, 60),
            RrdTimeframe::Week => (end - 3600 * 24 * 7, 30 * 60),
            RrdTimeframe::Month => (end - 3600 * 24 * 30, 30 * 60),
            RrdTimeframe::Year => (end - 3600 * 24 * 365, 6 * 60 * 60),
            RrdTimeframe::Decade => (end - 10 * 3600 * 24 * 366, 7 * 86400),
        };

        let cf = match mode {
            RrdMode::Max => AggregationFn::Maximum,
            RrdMode::Average => AggregationFn::Average,
        };

        self.cache
            .extract_cached_data(basedir, name, cf, resolution, Some(start), Some(end))
    }

    /// Update RRD Gauge values
    pub fn update_value(
        &self,
        name: &str,
        value: f64,
        timestamp: i64,
        datasource_type: DataSourceType,
    ) {
        if let Err(err) =
            self.cache
                .update_value_ignore_old(name, timestamp as f64, value, datasource_type)
        {
            log::error!("rrd::update_value '{name}' failed - {err}");
        }
    }
}
