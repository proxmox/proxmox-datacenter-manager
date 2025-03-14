use std::path::{Path, PathBuf};

use anyhow::Error;

use proxmox_sys::fs::CreateOptions;

/// Temporary file that be cleaned up when dropped.
pub struct NamedTempFile {
    path: PathBuf,
}

impl NamedTempFile {
    /// Create a new temporary file.
    ///
    /// The file will be created with the passed [`CreateOptions`].
    pub fn new(options: CreateOptions) -> Result<Self, Error> {
        let base = std::env::temp_dir().join("test");
        let (_, path) = proxmox_sys::fs::make_tmp_file(base, options)?;

        Ok(Self { path })
    }

    /// Return the [`Path`] to the temporary file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for NamedTempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Temporary directory that is cleaned up when dropped.
pub struct NamedTempDir {
    path: PathBuf,
}

impl NamedTempDir {
    /// Create a new temporary directory.
    ///
    /// The directory will be created with `0o700` permissions.
    pub fn new() -> Result<Self, Error> {
        let path = proxmox_sys::fs::make_tmp_dir("/tmp", None)?;

        Ok(Self { path })
    }

    /// Return the [`Path`] to the temporary directory.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for NamedTempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
