use std::{
    path::{Path, PathBuf},
    sync::{Once, OnceLock},
};

use anyhow::Context;
use serde::de::DeserializeOwned;

use proxmox_sys::fs::CreateOptions;

mod environment;
mod test_application;

pub use environment::TestRpcEnvironment;
pub use test_application::*;

/// Read and deserialize a previously captured API response.
pub async fn read_captured_response<T: DeserializeOwned, P: AsRef<Path>>(
    path: P,
) -> Result<T, proxmox_client::Error> {
    let s = tokio::fs::read_to_string(path.as_ref())
        .await
        .with_context(|| format!("could not read from {path}", path = path.as_ref().display()))
        .map_err(proxmox_client::Error::Anyhow)?;

    serde_json::from_str(&s).map_err(|e| proxmox_client::Error::Anyhow(e.into()))
}

static STATIC_TEMP_DIR: OnceLock<PathBuf> = OnceLock::new();

extern "C" fn cleanup() {
    if let Some(dir) = STATIC_TEMP_DIR.get() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Run commonly-used setup steps that must only run *once*.
///
/// Be sure to call this at the start of every test.
pub fn test_setup() {
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        let base = env!("CARGO_TARGET_TMPDIR");

        let file_opts = CreateOptions::new();

        let dir = proxmox_sys::fs::make_tmp_dir(base, None).unwrap();
        STATIC_TEMP_DIR.set(dir.clone()).unwrap();

        proxmox_rest_server::init_worker_tasks(dir.clone(), file_opts).unwrap();
        proxmox_access_control::init::init(&pdm_api_types::AccessControlConfig, dir)
            .expect("failed to setup access control config");

        unsafe {
            libc::atexit(cleanup);
        }
    });
}

/// Create a [`TestRpcEnvironment`] for the use in a test.
pub fn rpcenv() -> TestRpcEnvironment {
    TestRpcEnvironment
}
