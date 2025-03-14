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
