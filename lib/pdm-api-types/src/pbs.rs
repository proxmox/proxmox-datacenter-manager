use serde::{Deserialize, Serialize};

use proxmox_schema::api;

#[api]
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// Whether a task is still running.
pub enum IsRunning {
    /// Task is running.
    Running,
    /// Task is not running.
    Stopped,
}

// TODO: The pbs code should expose this via pbs-api-types!
#[api]
/// Status if a task.
#[derive(Debug, Deserialize, Serialize)]
pub struct TaskStatus {
    /// Exit status, if available.
    pub exitstatus: Option<String>,

    /// Task id.
    pub id: Option<String>,

    /// Node the task is running on.
    pub node: String,

    /// The Unix PID
    pub pid: i64,

    /// The task start time (Epoch)
    pub pstart: i64,

    /// The task's start time.
    pub starttime: i64,

    pub status: IsRunning,

    /// The task type.
    #[serde(rename = "type")]
    pub ty: String,

    /// The task's UPID.
    pub upid: String,

    /// The authenticated entity who started the task.
    pub user: String,
}

impl TaskStatus {
    /// Checks if the task is currently running.
    pub fn is_running(&self) -> bool {
        self.status == IsRunning::Running
    }
}
