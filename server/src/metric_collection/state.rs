use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::Error;
use serde::{Deserialize, Serialize};

use proxmox_sys::fs::CreateOptions;

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "kebab-case")]
/// Metric collection state file content.
struct State {
    remote_status: HashMap<String, RemoteStatus>,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "kebab-case")]
/// A remote's metric collection state.
pub struct RemoteStatus {
    /// Most recent datapoint - time stamp is based on remote time
    pub most_recent_datapoint: i64,
    /// Last successful metric collection - timestamp based on PDM's time
    pub last_collection: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Any error that occured during the last metric collection attempt.
    pub error: Option<String>,
}

/// Manage and persist metric collection state.
pub struct MetricCollectionState {
    /// Path to the persisted state
    path: PathBuf,
    /// File owner/perms for the persisted state file
    file_options: CreateOptions,
    /// The current state
    state: State,
}

impl MetricCollectionState {
    /// Initialize state by trying to load the existing statefile. If the file does not exist,
    /// state will be empty. If the file  failed to load, state will be empty and
    /// and error will be logged.
    pub fn new(statefile: PathBuf, file_options: CreateOptions) -> Self {
        let state = Self::load_or_default(&statefile)
            .inspect_err(|err| {
                log::error!("could not load metric collection state: {err}");
            })
            .unwrap_or_default();

        Self {
            path: statefile,
            file_options,
            state,
        }
    }

    /// Set a remote's status.
    pub fn set_status(&mut self, remote: String, remote_state: RemoteStatus) {
        self.state.remote_status.insert(remote, remote_state);
    }

    /// Get a remote's status.
    pub fn get_status(&self, remote: &str) -> Option<&RemoteStatus> {
        self.state.remote_status.get(remote)
    }

    /// Persist the state to the statefile.
    pub fn save(&self) -> Result<(), Error> {
        let data = serde_json::to_vec_pretty(&self.state)?;
        proxmox_sys::fs::replace_file(&self.path, &data, self.file_options, true)?;

        Ok(())
    }

    fn load_or_default(path: &Path) -> Result<State, Error> {
        let content = proxmox_sys::fs::file_read_optional_string(path)?;

        if let Some(content) = content {
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(Default::default())
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::metric_collection::collection_task::tests::get_create_options;
    use crate::test_support::temp::NamedTempFile;

    use super::*;

    #[test]
    fn save_and_load() -> Result<(), Error> {
        let file = NamedTempFile::new(get_create_options())?;
        let options = get_create_options();
        let mut state = MetricCollectionState::new(file.path().into(), options);

        state.set_status(
            "some-remote".into(),
            RemoteStatus {
                most_recent_datapoint: 1234,
                ..Default::default()
            },
        );

        state.save()?;

        let state = MetricCollectionState::new(file.path().into(), options);

        let status = state.get_status("some-remote").unwrap();
        assert_eq!(status.most_recent_datapoint, 1234);

        Ok(())
    }
}
