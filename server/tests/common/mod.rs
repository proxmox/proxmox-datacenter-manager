use std::path::{Path, PathBuf};
use std::sync::{Once, OnceLock};
use std::time::Duration;

use anyhow::{Context, Error, bail};
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
    TestRpcEnvironment {
        attribs: serde_json::json!({}),
    }
}

#[allow(unused)]
macro_rules! assert_http_error {
    ($result:expr, $status:expr $(, $expected:expr)? $(,)?) => {{
        let err = $result.unwrap_err();
        let http_err = err.downcast_ref::<proxmox_router::HttpError>().unwrap();

        assert_eq!(http_err.code, $status);

        $(
            let err_text = err.to_string();
            assert!(
                err_text.contains($expected),
                "'{}' not contained in '{}'",
                $expected,
                err_text,
            );
        )?
    }};
}

#[allow(unused)]
pub(crate) use assert_http_error;

/// Wait for a task to finish.
///
/// At the moment, this uses a hard-coded timeout of 5 seconds.
pub async fn wait_for_task(upid: &str) -> Result<String, Error> {
    let upid = upid.parse::<pdm_api_types::UPID>()?;

    for _ in 0..50 {
        let response =
            server::api::nodes::tasks::get_task_status(upid.clone(), &mut rpcenv()).await?;

        if response["status"].as_str().unwrap() != "running" {
            return Ok(response["exitstatus"]
                .as_str()
                .context("expected exitstatus to be a string")?
                .into());
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    bail!("worker did not finish after timeout");
}
