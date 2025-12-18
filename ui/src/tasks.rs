use proxmox_yew_comp::utils::{
    format_task_description, format_upid, register_pbs_tasks, register_pve_tasks,
    register_task_description,
};
use pwt::tr;

use pdm_api_types::{NativeUpid, RemoteUpid};
use yew::virtual_dom::Key;

pub fn register_tasks() {
    register_pve_tasks();
    register_pbs_tasks();
    register_pdm_tasks();
}

fn register_pdm_tasks() {
    register_task_description("logrotate", tr!("Log Rotation"));
    register_task_description(
        "refresh-remote-tasks",
        tr!("Fetch latest tasks from remotes"),
    );
    register_task_description(
        "refresh-remote-updates",
        tr!("Fetch system update list from remotes"),
    );
}

/// Format a UPID that is either [`RemoteUpid`] or a [`UPID`]
/// If it's a [`RemoteUpid`], prefixes it with the remote name
pub fn format_optional_remote_upid(upid: &str, include_remote: bool) -> String {
    if let Ok(remote_upid) = upid.parse::<RemoteUpid>() {
        let description = match remote_upid.native_upid() {
            Ok(NativeUpid::PveUpid(upid)) => {
                format_task_description(&upid.worker_type, upid.worker_id.as_deref())
            }
            Ok(NativeUpid::PbsUpid(upid)) => {
                format_task_description(&upid.worker_type, upid.worker_id.as_deref())
            }
            Err(_) => format_upid(remote_upid.upid()),
        };

        if include_remote {
            format!("{} - {}", remote_upid.remote(), description)
        } else {
            description
        }
    } else {
        format_upid(upid)
    }
}

/// Map worker types to sensible categories (that can also be used as filter for the api)
#[derive(Clone, PartialEq, PartialOrd, Eq, Ord)]
/// Map worker types to sensible categories
pub enum TaskWorkerType {
    Migrate,
    Qemu,
    Lxc,
    Ceph,
    Ha,
    Backup,
    Other(String),
    Remote(String),
}

/// Map a category from [`map_worker_type`] to a title text.
impl TaskWorkerType {
    /// Create new from a given worker type string, e.g. from a UPID
    pub fn new_from_str<A: AsRef<str>>(worker_type: A) -> Self {
        match worker_type.as_ref() {
            task_type if task_type.contains("migrate") => TaskWorkerType::Migrate,
            task_type if task_type.starts_with("qm") => TaskWorkerType::Qemu,
            task_type if task_type.starts_with("vz") && task_type != "vzdump" => {
                TaskWorkerType::Lxc
            }
            "vzdump" => TaskWorkerType::Backup,
            task_type if task_type.starts_with("ceph") => TaskWorkerType::Ceph,
            task_type if task_type.starts_with("ha") => TaskWorkerType::Ha,
            other => TaskWorkerType::Other(other.to_string()),
        }
    }

    /// Create title from the categories
    pub fn to_title(&self) -> String {
        match self {
            TaskWorkerType::Migrate => tr!("Guest Migrations"),
            TaskWorkerType::Qemu => tr!("Virtual Machine related Tasks"),
            TaskWorkerType::Lxc => tr!("Container related Tasks"),
            TaskWorkerType::Ceph => tr!("Ceph related Tasks"),
            TaskWorkerType::Ha => tr!("HA related Tasks"),
            TaskWorkerType::Backup => tr!("Backups and Backup Jobs"),
            TaskWorkerType::Other(other) => other.to_string(),
            TaskWorkerType::Remote(remote) => remote.to_string(),
        }
    }

    /// Create filter string for the api
    ///
    /// Note: The result has to be filtered with this again, since more records will be returned.
    /// E.g. using 'vz' will also return 'vzdump' tasks which are not desired.
    pub fn to_filter(&self) -> &str {
        match self {
            TaskWorkerType::Migrate => "migrate",
            TaskWorkerType::Qemu => "qm",
            TaskWorkerType::Lxc => "vz",
            TaskWorkerType::Ceph => "ceph",
            TaskWorkerType::Ha => "ha",
            TaskWorkerType::Backup => "vzdump",
            TaskWorkerType::Other(other) => other.as_str(),
            TaskWorkerType::Remote(remote) => remote.as_str(),
        }
    }
}

impl From<TaskWorkerType> for Key {
    fn from(value: TaskWorkerType) -> Self {
        match value {
            TaskWorkerType::Migrate => Key::from("migrate"),
            TaskWorkerType::Qemu => Key::from("qm"),
            TaskWorkerType::Lxc => Key::from("vz"),
            TaskWorkerType::Ceph => Key::from("ceph"),
            TaskWorkerType::Ha => Key::from("ha"),
            TaskWorkerType::Backup => Key::from("vzdump"),
            // use `__` prefix here so that they can't clash with the statically defined ones
            TaskWorkerType::Other(other) => Key::from(format!("__{other}")),
            TaskWorkerType::Remote(remote) => Key::from(format!("__remote_{remote}")),
        }
    }
}
