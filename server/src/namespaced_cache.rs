//! Generic namespaced cache implementation with optional value expiry.
//!
//! ```
//! use std::time::Duration;
//!
//! use proxmox_sys::fs::CreateOptions;
//! use server::namespaced_cache::NamespacedCache;
//!
//! let dir = tempfile::tempdir().unwrap();
//! let cache = NamespacedCache::new(dir.as_ref(), CreateOptions::new(), CreateOptions::new());
//! let write_guard = cache.write_blocking("remote-a", Duration::from_secs(1)).unwrap();
//! write_guard.set("val1", 1).unwrap();
//! assert_eq!(write_guard.get::<i32>("val1").unwrap().unwrap(), 1);
//!
//! ```

use std::fs::File;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::task::JoinError;

use proxmox_schema::api_types::SAFE_ID_REGEX;
use proxmox_sys::fs::CreateOptions;

/// Error type for [`NamespacedCache`].
#[derive(thiserror::Error, Debug)]
pub enum CacheError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("error: {0}")]
    Other(#[from] anyhow::Error),

    #[error("invalid key: '{0}'")]
    InvalidKey(String),

    #[error("invalid namespace: '{0}'")]
    InvalidNamespace(String),

    #[error("join error: {0}")]
    JoinError(#[from] JoinError),
}

/// A generic, namespaced cache with optional value expiration.
pub struct NamespacedCache {
    base_directory: PathBuf,
    dir_options: CreateOptions,
    file_options: CreateOptions,
}

impl NamespacedCache {
    /// Create a new cache instance.
    ///
    /// Cache entries will be persisted in the provided `base_directory`.
    /// `dir_options` are the [`CreateOptions`] used for the namespace directories, while `file_options`
    /// are the ones for persisted cache entries.
    pub fn new<P: Into<PathBuf>>(
        base_directory: P,
        dir_options: CreateOptions,
        file_options: CreateOptions,
    ) -> Self {
        Self {
            base_directory: base_directory.into(),
            dir_options,
            file_options,
        }
    }

    /// Lock a namespace for writing (blocking interface).
    ///
    /// This should *not* be called from async code. Use [`NamespacedCache::write`] instead.
    pub fn write_blocking(
        &self,
        namespace: &str,
        timeout: Duration,
    ) -> Result<BlockingWritableCacheNamespace, CacheError> {
        ensure_valid_namespace(namespace)?;
        let lock = self.lock_namespace_impl_blocking(namespace, timeout, true)?;

        Ok(BlockingWritableCacheNamespace {
            inner: WritableInner {
                _lock: lock,
                namespace: namespace.to_string(),
                base_path: self.base_directory.clone(),
                dir_options: self.dir_options,
                file_options: self.file_options,
            },
        })
    }

    /// Lock a namespace for writing (async interface).
    pub async fn write(
        &self,
        namespace: &str,
        timeout: Duration,
    ) -> Result<WritableCacheNamespace, CacheError> {
        ensure_valid_namespace(namespace)?;
        let lock = self.lock_namespace_impl(namespace, timeout, true).await?;

        Ok(WritableCacheNamespace {
            inner: Arc::new(WritableInner {
                _lock: lock,
                namespace: namespace.to_string(),
                base_path: self.base_directory.clone(),
                dir_options: self.dir_options,
                file_options: self.file_options,
            }),
        })
    }

    /// Lock a namespace for reading (blocking interface).
    ///
    /// This should *not* be called from async code. Use [`NamespacedCache::read`] instead.
    pub fn read_blocking(
        &self,
        namespace: &str,
        timeout: Duration,
    ) -> Result<BlockingReadableCacheNamespace, CacheError> {
        ensure_valid_namespace(namespace)?;
        let lock = self.lock_namespace_impl_blocking(namespace, timeout, false)?;

        Ok(BlockingReadableCacheNamespace {
            inner: ReadableInner {
                _lock: lock,
                namespace: namespace.to_string(),
                base_path: self.base_directory.clone(),
            },
        })
    }

    /// Lock a namespace for reading (async interface).
    pub async fn read(
        &self,
        namespace: &str,
        timeout: Duration,
    ) -> Result<ReadableCacheNamespace, CacheError> {
        ensure_valid_namespace(namespace)?;
        let lock = self.lock_namespace_impl(namespace, timeout, false).await?;

        Ok(ReadableCacheNamespace {
            inner: Arc::new(ReadableInner {
                _lock: lock,
                namespace: namespace.to_string(),
                base_path: self.base_directory.clone(),
            }),
        })
    }

    async fn lock_namespace_impl(
        &self,
        namespace: &str,
        timeout: Duration,
        exclusive: bool,
    ) -> Result<File, CacheError> {
        let path = get_lockfile(&self.base_directory, namespace);
        let file_options = self.file_options;
        let lock = tokio::task::spawn_blocking(move || {
            proxmox_sys::fs::open_file_locked(&path, timeout, exclusive, file_options)
        })
        .await??;

        Ok(lock)
    }

    fn lock_namespace_impl_blocking(
        &self,
        namespace: &str,
        timeout: Duration,
        exclusive: bool,
    ) -> Result<File, CacheError> {
        let path = get_lockfile(&self.base_directory, namespace);
        let lock = proxmox_sys::fs::open_file_locked(&path, timeout, exclusive, self.file_options)?;

        Ok(lock)
    }
}

/// A readable cache namespace (blocking interface).
pub struct BlockingReadableCacheNamespace {
    inner: ReadableInner,
}

/// A readable cache namespace (async interface).
pub struct ReadableCacheNamespace {
    inner: Arc<ReadableInner>,
}

struct ReadableInner {
    _lock: File,
    namespace: String,
    base_path: PathBuf,
}

impl BlockingReadableCacheNamespace {
    /// Read a value from the cache.
    ///
    /// # Errors:
    ///   - The file associated with this key could not be read
    ///   - The file could not be deserialized (e.g. invalid format)
    pub fn get<T: Serialize + DeserializeOwned>(&self, key: &str) -> Result<Option<T>, CacheError> {
        get_impl(&self.inner.base_path, &self.inner.namespace, key, None)
    }

    /// Read a value from the cache, given a maximum age of the cache entry.
    ///
    /// # Errors:
    ///   - The file associated with this key could not be read
    ///   - The file could not be deserialized (e.g. invalid format)
    pub fn get_with_max_age<T: Serialize + DeserializeOwned>(
        &self,
        key: &str,
        max_age: i64,
    ) -> Result<Option<T>, CacheError> {
        get_impl(
            &self.inner.base_path,
            &self.inner.namespace,
            key,
            Some(max_age),
        )
    }
}

impl ReadableCacheNamespace {
    /// Read a value from the cache.
    ///
    /// # Errors:
    ///   - The file associated with this key could not be read
    ///   - The file could not be deserialized (e.g. invalid format)
    pub async fn get<T: Serialize + DeserializeOwned + Send + 'static>(
        &self,
        key: &str,
    ) -> Result<Option<T>, CacheError> {
        let key = key.to_string();
        let inner = Arc::clone(&self.inner);

        tokio::task::spawn_blocking(move || {
            get_impl(&inner.base_path, &inner.namespace, &key, None)
        })
        .await?
    }

    /// Read a value from the cache, given a maximum age of the cache entry.
    ///
    /// # Errors:
    ///   - The file associated with this key could not be read
    ///   - The file could not be deserialized (e.g. invalid format)
    pub async fn get_with_max_age<T: Serialize + DeserializeOwned + Send + 'static>(
        &self,
        key: &str,
        max_age: i64,
    ) -> Result<Option<T>, CacheError> {
        let key = key.to_string();
        let inner = Arc::clone(&self.inner);

        tokio::task::spawn_blocking(move || {
            get_impl(&inner.base_path, &inner.namespace, &key, Some(max_age))
        })
        .await?
    }
}

/// A writable cache namespace (blocking interface).
pub struct BlockingWritableCacheNamespace {
    inner: WritableInner,
}

/// A writable cache namespace (async interface).
pub struct WritableCacheNamespace {
    inner: Arc<WritableInner>,
}

struct WritableInner {
    _lock: File,
    namespace: String,
    base_path: PathBuf,
    dir_options: CreateOptions,
    file_options: CreateOptions,
}

impl BlockingWritableCacheNamespace {
    /// Remove a cache entry.
    ///
    /// This returns `Ok(())` if the key does not exist.
    ///
    /// # Errors:
    ///   - The file could not be deleted due to insufficient privileges.
    pub fn remove(&self, key: &str) -> Result<(), CacheError> {
        remove_impl(&self.inner, key)
    }

    /// Set a cache entry.
    ///
    /// # Errors
    ///   - `value` could not be serialized
    ///   - The namespace directory could not be created
    ///   - The cache file could not be written to or atomically replaced
    pub fn set<T: Serialize + DeserializeOwned>(
        &self,
        key: &str,
        value: T,
    ) -> Result<(), CacheError> {
        set_impl(&self.inner, key, value, proxmox_time::epoch_i64())
    }

    /// Set a cache entry, but only if the timestamp is more recent than the already existing
    /// entry.
    ///
    /// If the existing entry is newer, it will be returned as `Ok(Some(existing_entry))`.
    /// If the entry does not exist yet, the entry is always set.
    /// On a successful write, `Ok(None)` is returned.
    ///
    /// # Errors
    ///   - `value` could not be serialized
    ///   - The namespace directory could not be created
    ///   - The cache file could not be written to or atomically replaced
    pub fn set_if_newer<T: Serialize + DeserializeOwned>(
        &self,
        key: &str,
        value: T,
    ) -> Result<Option<T>, CacheError> {
        set_if_newer_impl(&self.inner, key, value, proxmox_time::epoch_i64())
    }

    /// Set a cache entry with an explicitly provided timestamp.
    ///
    /// # Errors
    ///   - `value` could not be serialized
    ///   - The namespace directory could not be created
    ///   - The cache file could not be written to or atomically replaced
    pub fn set_with_timestamp<T: Serialize + DeserializeOwned>(
        &self,
        key: &str,
        value: T,
        timestamp: i64,
    ) -> Result<(), CacheError> {
        set_impl(&self.inner, key, value, timestamp)
    }

    /// Set a cache entry with an explicitly provided timestamp, but only if the timestamp is more
    /// recent than the already existing entry.
    ///
    /// If the existing entry is newer, it will be returned as `Ok(Some(existing_entry))`.
    /// If the entry does not exist yet, the entry is always set.
    /// On a successful write, `Ok(None)` is returned.
    ///
    /// # Errors
    ///   - `value` could not be serialized
    ///   - The namespace directory could not be created
    ///   - The cache file could not be written to or atomically replaced
    pub fn set_if_newer_with_timestamp<T: Serialize + DeserializeOwned>(
        &self,
        key: &str,
        value: T,
        timestamp: i64,
    ) -> Result<Option<T>, CacheError> {
        set_if_newer_impl(&self.inner, key, value, timestamp)
    }

    /// Read a value from the cache.
    ///
    /// # Errors:
    ///   - The file associated with this key could not be read
    ///   - The file could not be deserialized (e.g. invalid format)
    pub fn get<T: Serialize + DeserializeOwned>(&self, key: &str) -> Result<Option<T>, CacheError> {
        get_impl(&self.inner.base_path, &self.inner.namespace, key, None)
    }

    /// Read a value from the cache, given a maximum age of the cache entry.
    ///
    /// # Errors:
    ///   - The file associated with this key could not be read
    ///   - The file could not be deserialized (e.g. invalid format)
    pub fn get_with_max_age<T: Serialize + DeserializeOwned>(
        &self,
        key: &str,
        max_age: i64,
    ) -> Result<Option<T>, CacheError> {
        get_impl(
            &self.inner.base_path,
            &self.inner.namespace,
            key,
            Some(max_age),
        )
    }
}

impl WritableCacheNamespace {
    /// Remove a cache entry.
    ///
    /// This returns `Ok(())` if the key does not exist.
    ///
    /// # Errors:
    ///   - The file could not be deleted due to insufficient privileges.
    pub async fn remove(&self, key: &str) -> Result<(), CacheError> {
        let inner = Arc::clone(&self.inner);
        let key = key.to_string();

        tokio::task::spawn_blocking(move || remove_impl(&inner, &key)).await?
    }

    /// Set a cache entry.
    ///
    /// # Errors
    ///   - `value` could not be serialized
    ///   - The namespace directory could not be created
    ///   - The cache file could not be written to or atomically replaced
    pub async fn set<T: Serialize + DeserializeOwned + Send + 'static>(
        &self,
        key: &str,
        value: T,
    ) -> Result<(), CacheError> {
        let inner = Arc::clone(&self.inner);
        let key = key.to_string();

        tokio::task::spawn_blocking(move || {
            set_impl(&inner, &key, value, proxmox_time::epoch_i64())
        })
        .await?
    }

    /// Set a cache entry, but only if the timestamp is more recent than the already existing
    /// entry.
    ///
    /// If the existing entry is newer, it will be returned as `Ok(Some(existing_entry))`.
    /// If the entry does not exist yet, the entry is always set.
    /// On a successful write, `Ok(None)` is returned.
    ///
    /// # Errors
    ///   - `value` could not be serialized
    ///   - The namespace directory could not be created
    ///   - The cache file could not be written to or atomically replaced
    pub async fn set_if_newer<T: Serialize + DeserializeOwned + Send + 'static>(
        &self,
        key: &str,
        value: T,
    ) -> Result<Option<T>, CacheError> {
        let inner = Arc::clone(&self.inner);
        let key = key.to_string();

        tokio::task::spawn_blocking(move || {
            set_if_newer_impl(&inner, &key, value, proxmox_time::epoch_i64())
        })
        .await?
    }

    /// Set a cache entry with an explicitly provided timestamp.
    ///
    /// # Errors
    ///   - `value` could not be serialized
    ///   - The namespace directory could not be created
    ///   - The cache file could not be written to or atomically replaced
    pub async fn set_with_timestamp<T: Serialize + DeserializeOwned + Send + 'static>(
        &self,
        key: &str,
        value: T,
        timestamp: i64,
    ) -> Result<(), CacheError> {
        let inner = Arc::clone(&self.inner);
        let key = key.to_string();
        tokio::task::spawn_blocking(move || set_impl(&inner, &key, value, timestamp)).await?
    }

    /// Set a cache entry with an explicitly provided timestamp, but only if the timestamp is more
    /// recent than the already existing entry.
    ///
    /// If the existing entry is newer, it will be returned as `Ok(Some(existing_entry))`.
    /// If the entry does not exist yet, the entry is always set.
    /// On a successful write, `Ok(None)` is returned.
    ///
    /// # Errors
    ///   - `value` could not be serialized
    ///   - The namespace directory could not be created
    ///   - The cache file could not be written to or atomically replaced
    pub async fn set_if_newer_with_timestamp<T: Serialize + DeserializeOwned + Send + 'static>(
        &self,
        key: &str,
        value: T,
        timestamp: i64,
    ) -> Result<Option<T>, CacheError> {
        let inner = Arc::clone(&self.inner);
        let key = key.to_string();
        tokio::task::spawn_blocking(move || set_if_newer_impl(&inner, &key, value, timestamp))
            .await?
    }

    /// Read a value from the cache.
    ///
    /// # Errors:
    ///   - The file associated with this key could not be read
    ///   - The file could not be deserialized (e.g. invalid format)
    pub async fn get<T: Serialize + DeserializeOwned + Send + 'static>(
        &self,
        key: &str,
    ) -> Result<Option<T>, CacheError> {
        let key = key.to_string();
        let inner = Arc::clone(&self.inner);

        tokio::task::spawn_blocking(move || {
            get_impl(&inner.base_path, &inner.namespace, &key, None)
        })
        .await?
    }

    /// Read a value from the cache, given a maximum age of the cache entry.
    ///
    /// # Errors:
    ///   - The file associated with this key could not be read
    ///   - The file could not be deserialized (e.g. invalid format)
    pub async fn get_with_max_age<T: Serialize + DeserializeOwned + Send + 'static>(
        &self,
        key: &str,
        max_age: i64,
    ) -> Result<Option<T>, CacheError> {
        let key = key.to_string();
        let inner = Arc::clone(&self.inner);

        tokio::task::spawn_blocking(move || {
            get_impl(&inner.base_path, &inner.namespace, &key, Some(max_age))
        })
        .await?
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct CacheEntry<T> {
    timestamp: i64,
    value: T,
}

impl<T> CacheEntry<T> {
    fn is_expired(&self, now: i64, max_age: i64) -> bool {
        if max_age == 0 {
            return true;
        }

        let diff = now - self.timestamp;
        diff >= max_age || diff < 0
    }
}

fn get_impl<T: Serialize + DeserializeOwned>(
    base: &Path,
    namespace: &str,
    key: &str,
    max_age: Option<i64>,
) -> Result<Option<T>, CacheError> {
    // Namespace should already be verified at this point, no point in checking it again.
    ensure_valid_key(key)?;

    let path = get_path(base, namespace, key);

    Ok(get_from_path(&path, max_age)?.map(|a| a.value))
}

fn get_from_path<T: Serialize + DeserializeOwned>(
    path: &Path,
    max_age: Option<i64>,
) -> Result<Option<CacheEntry<T>>, CacheError> {
    let content = proxmox_sys::fs::file_read_optional_string(path)?;

    if let Some(content) = content {
        let val = serde_json::from_str::<CacheEntry<T>>(&content)?;

        if let Some(max_age) = max_age {
            if val.is_expired(proxmox_time::epoch_i64(), max_age) {
                return Ok(None);
            }
        }
        return Ok(Some(val));
    }

    Ok(None)
}

fn set_if_newer_impl<T: Serialize + DeserializeOwned>(
    inner: &WritableInner,
    key: &str,
    value: T,
    timestamp: i64,
) -> Result<Option<T>, CacheError> {
    ensure_valid_key(key)?;
    let path = get_path(&inner.base_path, &inner.namespace, key);

    match get_from_path(&path, None) {
        Ok(Some(existing)) => {
            if existing.timestamp > timestamp {
                return Ok(Some(existing.value));
            }
        }
        Ok(None) => {}
        Err(CacheError::Serde(err)) => {
            // Special case, only log deserialization errors, in that case we want to override
            // the cache file anyways.
            log::error!("could not deserialize existing cache file in set_if_newer, overwriting anyways: {err}");
        }
        Err(err) => {
            // Any other error will be bubbled up
            return Err(err);
        }
    }

    proxmox_sys::fs::create_path(
        path.parent().unwrap(),
        Some(inner.dir_options),
        Some(inner.dir_options),
    )?;

    let entry = CacheEntry { timestamp, value };

    let data = serde_json::to_vec(&entry)?;
    proxmox_sys::fs::replace_file(path, &data, inner.file_options, true)?;

    Ok(None)
}

fn set_impl<T: Serialize + DeserializeOwned>(
    inner: &WritableInner,
    key: &str,
    value: T,
    timestamp: i64,
) -> Result<(), CacheError> {
    ensure_valid_key(key)?;
    let path = get_path(&inner.base_path, &inner.namespace, key);

    proxmox_sys::fs::create_path(
        path.parent().unwrap(),
        Some(inner.dir_options),
        Some(inner.dir_options),
    )?;

    let entry = CacheEntry { timestamp, value };

    let data = serde_json::to_vec(&entry)?;
    proxmox_sys::fs::replace_file(path, &data, inner.file_options, true)?;

    Ok(())
}

fn remove_impl(inner: &WritableInner, key: &str) -> Result<(), CacheError> {
    ensure_valid_key(key)?;
    let path = get_path(&inner.base_path, &inner.namespace, key);

    if let Err(err) = std::fs::remove_file(path) {
        if err.kind() == ErrorKind::NotFound {
            return Ok(());
        }

        return Err(err.into());
    }

    Ok(())
}

fn get_path(base: &Path, namespace: &str, key: &str) -> PathBuf {
    let path = base.join(namespace).join(format!("{key}.json"));
    path
}

fn get_lockfile(base: &Path, namespace: &str) -> PathBuf {
    let path = base.join(format!(".{namespace}.lock"));
    path
}

/// Make sure that an identifier is safe to use as a cache key.
fn ensure_valid_key(key: &str) -> Result<(), CacheError> {
    if !SAFE_ID_REGEX.is_match(key) {
        return Err(CacheError::InvalidKey(key.into()));
    }

    Ok(())
}

/// Make sure that an identifier is safe to use as a namespace.
fn ensure_valid_namespace(namespace: &str) -> Result<(), CacheError> {
    if !SAFE_ID_REGEX.is_match(namespace) {
        return Err(CacheError::InvalidNamespace(namespace.into()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    const TIMEOUT: Duration = Duration::from_secs(1);

    fn make_cache() -> (TempDir, NamespacedCache) {
        let dir = tempfile::tempdir().unwrap();

        let cache = NamespacedCache::new(dir.as_ref(), CreateOptions::new(), CreateOptions::new());

        (dir, cache)
    }

    #[test]
    fn test_cache() {
        let (_dir, cache) = make_cache();

        let write_guard = cache.write_blocking("remote-a", TIMEOUT).unwrap();
        write_guard.set("val1", 1).unwrap();
        write_guard.set("val2", 1).unwrap();

        assert_eq!(write_guard.get::<i32>("val1").unwrap().unwrap(), 1);

        write_guard.remove("val1").unwrap();
        assert!(write_guard.get::<String>("val1").unwrap().is_none());

        drop(write_guard);

        let read_guard = cache.read_blocking("remote-a", TIMEOUT).unwrap();

        assert_eq!(read_guard.get::<i32>("val2").unwrap().unwrap(), 1);
    }

    #[test]
    fn test_remove_nonexisting() {
        let (_dir, cache) = make_cache();

        let a = cache.write_blocking("remote-a", TIMEOUT).unwrap();

        // Deleting a key that does not exist is okay and should not error.
        assert!(a.remove("val").is_ok());
    }

    #[test]
    fn test_remove_failure() {
        let (dir, cache) = make_cache();

        let a = cache.write_blocking("remote-a", TIMEOUT).unwrap();
        // Triggering a general failure by generating a directory that conflicts with the cache key
        std::fs::create_dir_all(dir.path().join("remote-a").join("val.json")).unwrap();
        assert!(a.remove("val").is_err());
    }

    #[test]
    fn test_get_with_max_age() {
        let (_dir, cache) = make_cache();

        let write_guard = cache.write_blocking("remote-a", TIMEOUT).unwrap();

        let now = proxmox_time::epoch_i64();

        write_guard
            .set_with_timestamp("somekey", 1, now - 1000)
            .unwrap();

        assert!(write_guard
            .get_with_max_age::<i32>("somekey", 999)
            .unwrap()
            .is_none());

        drop(write_guard);
        let read_guard = cache.read_blocking("remote-a", TIMEOUT).unwrap();
        assert!(read_guard
            .get_with_max_age::<i32>("somekey", 999)
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_set_if_newer_with_timestamp() {
        let (_dir, cache) = make_cache();

        let guard = cache.write_blocking("remote-a", TIMEOUT).unwrap();

        let now = proxmox_time::epoch_i64() + 200;

        assert!(guard
            .set_if_newer_with_timestamp("somekey", 1, now)
            .unwrap()
            .is_none());

        assert_eq!(guard.get::<i32>("somekey").unwrap().unwrap(), 1);
        assert!(guard
            .set_if_newer_with_timestamp("somekey", 2, now + 1)
            .unwrap()
            .is_none());
        assert_eq!(guard.get::<i32>("somekey").unwrap().unwrap(), 2);
        assert!(matches!(
            guard
                .set_if_newer_with_timestamp("somekey", 3, now)
                .unwrap(),
            Some(2)
        ));
        // This should still contain the old value.
        assert_eq!(guard.get::<i32>("somekey").unwrap().unwrap(), 2);

        assert!(matches!(guard.set_if_newer("somekey", 3).unwrap(), Some(2)));
        // This should still contain the old value.
        assert_eq!(guard.get::<i32>("somekey").unwrap().unwrap(), 2);
    }

    #[test]
    fn test_expiration() {
        let entry = CacheEntry {
            value: (),
            timestamp: 1000,
        };

        assert!(!entry.is_expired(1000, 100));
        assert!(!entry.is_expired(1099, 100));
        assert!(entry.is_expired(1100, 100));
        assert!(entry.is_expired(1101, 100));

        // if max-age is 0, the entry is never fresh
        assert!(entry.is_expired(1000, 0));
    }

    #[test]
    fn test_invalid_namespaces() {
        let (_dir, cache) = make_cache();

        for id in [
            "../remote-a",
            "remote-a/../something",
            "remote-a/../",
            "../",
        ] {
            assert!(matches!(
                cache.write_blocking(id, TIMEOUT),
                Err(CacheError::InvalidNamespace(_))
            ));
            assert!(matches!(
                cache.read_blocking(id, TIMEOUT),
                Err(CacheError::InvalidNamespace(_))
            ));
        }
    }

    #[test]
    fn test_invalid_keys() {
        let (_dir, cache) = make_cache();

        let write_guard = cache.write_blocking("remote-a", TIMEOUT).unwrap();
        let read_guard = cache.write_blocking("remote-b", TIMEOUT).unwrap();

        for id in ["../somekey", "somekey/../something", "somekey/../", "../"] {
            assert!(matches!(
                write_guard.set(id, ()),
                Err(CacheError::InvalidKey(_))
            ));
            assert!(matches!(
                write_guard.get::<()>(id),
                Err(CacheError::InvalidKey(_))
            ));
            assert!(matches!(
                write_guard.get_with_max_age::<()>(id, 1000),
                Err(CacheError::InvalidKey(_))
            ));
            assert!(matches!(
                write_guard.remove(id),
                Err(CacheError::InvalidKey(_))
            ));
            assert!(matches!(
                read_guard.get::<()>(id),
                Err(CacheError::InvalidKey(_))
            ));
            assert!(matches!(
                read_guard.get_with_max_age::<()>(id, 1000),
                Err(CacheError::InvalidKey(_))
            ));
        }
    }

    #[tokio::test]
    async fn test_async() {
        let (_dir, cache) = make_cache();

        let lock = cache.write("some-remote", TIMEOUT).await.unwrap();

        lock.set("somekey", 1234).await.unwrap();
        assert_eq!(lock.get::<i32>("somekey").await.unwrap(), Some(1234));
        lock.remove("somekey").await.unwrap();
        assert!(lock.get::<i32>("somekey").await.unwrap().is_none());

        let now = proxmox_time::epoch_i64() - 1000;

        lock.set_with_timestamp("somekey", 1234, now).await.unwrap();

        assert!(lock
            .get_with_max_age::<i32>("somekey", 900)
            .await
            .unwrap()
            .is_none());

        drop(lock);

        let lock = cache.read("some-remote", TIMEOUT).await.unwrap();
        assert_eq!(lock.get::<i32>("somekey").await.unwrap(), Some(1234));
        assert!(lock
            .get_with_max_age::<i32>("somekey", 900)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn test_async_set_if_newer() {
        let (_dir, cache) = make_cache();

        let lock = cache.write("some-remote", TIMEOUT).await.unwrap();

        let now = proxmox_time::epoch_i64() + 1000;
        lock.set_with_timestamp("somekey", 1234, now).await.unwrap();

        // This one should not set the entry, the existing timestamp is more recent
        lock.set_if_newer_with_timestamp("somekey", 1235, now - 1)
            .await
            .unwrap();

        assert_eq!(lock.get::<i32>("somekey").await.unwrap(), Some(1234));

        // this should not change the entry
        lock.set_if_newer("otherkey", 1235).await.unwrap();
        assert_eq!(lock.get::<i32>("somekey").await.unwrap(), Some(1234));
    }
}
