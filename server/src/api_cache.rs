//! Cache for API responses from remotes.
//!
//! This cache is namespaced by remote and also offers a 'global' namespace
//! for values that are valid across remotes (e.g. aggregations).
//!
//! The cache has both, a blocking, as well as an async interface that can be used to get or set
//! cache entries.
//!
//! A namespace (so, either the remote one's or the global one) must be locked before it can be
//! accessed. All locking functions use a 10 second timeout while waiting for the lock.
//!
//! ## Blocking interface
//!   - [`read_remote_blocking`]
//!   - [`write_remote_blocking`]
//!   - [`read_global_blocking`]
//!   - [`write_global_blocking`]
//!
//! These functions return [`BlockingReadableCacheNamespace`] and [`BlockingWritableCacheNamespace`], respectively.
//! Both only offer blocking operations for interacting with the entries of the locked namespace.
//!
//! ## `async` interface
//!   - [`read_remote`]
//!   - [`write_remote`]
//!   - [`read_global`]
//!   - [`write_global`]
//!
//! These functions return [`ReadableCacheNamespace`] and [`WritableCacheNamespace`], respectively.
//! Both offer an async wrapper for interacting with the entries of the locked namespace.
//!
//! ```no_run
//! use server::api_cache;
//!
//! #[derive(serde::Serialize, serde::Deserialize)]
//! struct CacheableData {
//!     id: String,
//! }
//!
//! let data = CacheableData {
//!     id: "some-id".to_string(),
//! };
//!
//! // Lock the cache namespace for 'some-remote' for write access
//! let lock = api_cache::write_remote_blocking("some-remote").unwrap();
//!
//! // Set some value (must be Serialize + Deserialize)
//! lock.set("some-key", data).unwrap();
//!
//! // Retrieve the cached entry
//! let data: Option<CacheableData> = lock.get("some-key").unwrap();
//!
//! // Remove the cached entry
//! lock.remove("some-key").unwrap();
//!
//! ```

use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;

use nix::sys::stat::Mode;

use crate::namespaced_cache::{
    BlockingReadableCacheNamespace, BlockingWritableCacheNamespace, CacheError, NamespacedCache,
    ReadableCacheNamespace, WritableCacheNamespace,
};

/// Path at which API responses are cached.
pub const PDM_API_CACHE_PATH: &str = concat!(pdm_buildcfg::PDM_RUN_DIR_M!(), "/api-cache");

const GLOBAL_NAMESPACE: &str = "global";
const LOCK_TIMEOUT: Duration = Duration::from_secs(10);

static CACHE: LazyLock<NamespacedCache> = LazyLock::new(|| {
    let file_options = proxmox_product_config::default_create_options();
    let dir_options = file_options.perm(Mode::from_bits_truncate(0o750));

    NamespacedCache::new(PathBuf::from(PDM_API_CACHE_PATH), dir_options, file_options)
});

fn format_remote_namespace(remote: &str) -> String {
    format!("remote-{remote}")
}

/// Lock the cache for reading remote-specific data (blocking interface).
pub fn read_remote_blocking(remote: &str) -> Result<BlockingReadableCacheNamespace, CacheError> {
    CACHE.read_blocking(&format_remote_namespace(remote), LOCK_TIMEOUT)
}

/// Lock the cache for writing remote-specific data (blocking interface).
pub fn write_remote_blocking(remote: &str) -> Result<BlockingWritableCacheNamespace, CacheError> {
    CACHE.write_blocking(&format_remote_namespace(remote), LOCK_TIMEOUT)
}

/// Lock the cache for reading global data (blocking interface).
pub fn read_global_blocking() -> Result<BlockingReadableCacheNamespace, CacheError> {
    CACHE.read_blocking(GLOBAL_NAMESPACE, LOCK_TIMEOUT)
}

/// Lock the cache for writing global data (blocking interface).
pub fn write_global_blocking() -> Result<BlockingWritableCacheNamespace, CacheError> {
    CACHE.write_blocking(GLOBAL_NAMESPACE, LOCK_TIMEOUT)
}

/// Lock the cache for reading remote-specific data (async interface).
pub async fn read_remote(remote: &str) -> Result<ReadableCacheNamespace, CacheError> {
    CACHE
        .read(&format_remote_namespace(remote), LOCK_TIMEOUT)
        .await
}

/// Lock the cache for writing remote-specific data (async interface).
pub async fn write_remote(remote: &str) -> Result<WritableCacheNamespace, CacheError> {
    CACHE
        .write(&format_remote_namespace(remote), LOCK_TIMEOUT)
        .await
}

/// Lock the cache for reading global data (async interface).
pub async fn read_global() -> Result<ReadableCacheNamespace, CacheError> {
    CACHE.read(GLOBAL_NAMESPACE, LOCK_TIMEOUT).await
}

/// Lock the cache for writing global data (async interface).
pub async fn write_global() -> Result<WritableCacheNamespace, CacheError> {
    CACHE.write(GLOBAL_NAMESPACE, LOCK_TIMEOUT).await
}
