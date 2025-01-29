use anyhow::{bail, format_err, Error};
use http::request::Parts;
use http::{header, Response, StatusCode};
use hyper::Body;
use serde_json::{json, Value};

use proxmox_access_control::CachedUserInfo;
use proxmox_async::stream::AsyncReaderStream;
use proxmox_rest_server::{upid_log_path, upid_read_status, TaskState};
use proxmox_router::{list_subdirs_api_method, Permission, Router, RpcEnvironment, SubdirMap};
use proxmox_schema::{api, Schema};
use proxmox_sortable_macro::sortable;

use pdm_api_types::{
    Authid, TaskFilters, TaskListItem, TaskStateType, Tokenname, Userid, NODE_SCHEMA,
    PRIV_SYS_AUDIT, PRIV_SYS_MODIFY, UPID, UPID_SCHEMA,
};

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_TASKS)
    .match_all("upid", &UPID_API_ROUTER);

pub const UPID_API_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(UPID_API_SUBDIRS))
    .delete(&API_METHOD_STOP_TASK)
    .subdirs(UPID_API_SUBDIRS);

#[sortable]
const UPID_API_SUBDIRS: SubdirMap = &sorted!([
    ("log", &Router::new().get(&API_METHOD_READ_TASK_LOG)),
    ("status", &Router::new().get(&API_METHOD_GET_TASK_STATUS))
]);

fn check_task_access(auth_id: &Authid, upid: &UPID) -> Result<(), Error> {
    let task_auth_id: Authid = upid.auth_id.parse()?;
    if auth_id == &task_auth_id
        || (task_auth_id.is_token() && &Authid::from(task_auth_id.user().clone()) == auth_id)
    {
        // task owner can always read
        Ok(())
    } else {
        let user_info = CachedUserInfo::new()?;

        // access to all tasks
        // or task == job which the user/token could have configured/manually executed

        user_info
            .check_privs(auth_id, &["system", "tasks"], PRIV_SYS_AUDIT, false)
            .or_else(|_| bail!("task access not allowed"))
    }
}

#[api(
    serializing: true,
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA
            },
            filters: {
                type: TaskFilters,
                flatten: true,
            },
        },
    },
    returns: pdm_api_types::NODE_TASKS_LIST_TASKS_RETURN_TYPE,
    access: {
        description: "Users can only see their own tasks, unless they have Sys.Audit on /system/tasks.",
        permission: &Permission::Anybody,
    },
)]
/// List tasks.
#[allow(clippy::too_many_arguments)]
pub fn list_tasks(
    filters: TaskFilters,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<TaskListItem>, Error> {
    let TaskFilters {
        start,
        limit,
        errors,
        running,
        userfilter,
        since,
        until,
        typefilter,
        statusfilter,
    } = filters;

    let auth_id: Authid = rpcenv.get_auth_id().unwrap().parse()?;
    let user_info = CachedUserInfo::new()?;
    let user_privs = user_info.lookup_privs(&auth_id, &["system", "tasks"]);

    let list_all = (user_privs & PRIV_SYS_AUDIT) != 0;

    let list = proxmox_rest_server::TaskListInfoIterator::new(running)?;
    let limit = if limit > 0 {
        limit as usize
    } else {
        usize::MAX
    };

    let mut skipped = 0;
    let mut result: Vec<TaskListItem> = Vec::new();

    for info in list {
        let info = match info {
            Ok(info) => info,
            Err(_) => break,
        };

        if let Some(until) = until {
            if info.upid.starttime > until {
                continue;
            }
        }

        if let Some(since) = since {
            if let Some(ref state) = info.state {
                if state.endtime() < since {
                    // we reached the tasks that ended before our 'since' so we can stop iterating
                    break;
                }
            }
            if info.upid.starttime < since {
                continue;
            }
        }

        if !list_all && check_task_access(&auth_id, &info.upid).is_err() {
            continue;
        }

        if let Some(needle) = &userfilter {
            if !info.upid.auth_id.to_string().contains(needle) {
                continue;
            }
        }

        if let Some(typefilter) = &typefilter {
            if !info.upid.worker_type.contains(typefilter) {
                continue;
            }
        }

        match (&info.state, &statusfilter) {
            (Some(_), _) if running => continue,
            (Some(TaskState::OK { .. }), _) if errors => continue,
            (Some(state), Some(filters)) => {
                if !filters.contains(&tasktype(state)) {
                    continue;
                }
            }
            (None, Some(_)) => continue,
            _ => {}
        }

        if skipped < start as usize {
            skipped += 1;
            continue;
        }

        result.push(into_task_list_item(info));

        if result.len() >= limit {
            break;
        }
    }

    let mut count = result.len() + start as usize;
    if !result.is_empty() && result.len() >= limit {
        // we have a 'virtual' entry as long as we have any new
        count += 1;
    }

    rpcenv["total"] = Value::from(count);

    Ok(result)
}

#[api(
    protected: true,
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
            upid: { type: UPID },
        },
    },
    access: {
        description: "Users can stop their own tasks, or need Sys.Modify on /system/tasks.",
        permission: &Permission::Anybody,
    },
)]
/// Try to stop a task.
fn stop_task(upid: UPID, rpcenv: &mut dyn RpcEnvironment) -> Result<(), Error> {
    let auth_id = rpcenv.get_auth_id().unwrap();

    if auth_id != upid.auth_id {
        let user_info = CachedUserInfo::new()?;
        let auth_id: Authid = auth_id.parse()?;
        user_info.check_privs(&auth_id, &["system", "tasks"], PRIV_SYS_MODIFY, false)?;
    }

    proxmox_rest_server::abort_worker_nowait(upid);

    Ok(())
}

#[api(
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
            upid: {
                schema: UPID_SCHEMA,
            },
        },
    },
    returns: {
        description: "Task status information.",
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
            upid: {
                schema: UPID_SCHEMA,
            },
            pid: {
                type: i64,
                description: "The Unix PID.",
            },
            pstart: {
                type: u64,
                description: "The Unix process start time from `/proc/pid/stat`",
            },
            starttime: {
                type: i64,
                description: "The task start time (Epoch)",
            },
            "type": {
                type: String,
                description: "Worker type (arbitrary ASCII string)",
            },
            id: {
                type: String,
                optional: true,
                description: "Worker ID (arbitrary ASCII string)",
            },
            user: {
                type: Userid,
            },
            tokenid: {
                type: Tokenname,
                optional: true,
            },
            status: {
                type: String,
                description: "'running' or 'stopped'",
            },
            exitstatus: {
                type: String,
                optional: true,
                description: "'OK', 'Error: <msg>', or 'unknown'.",
            },
        },
    },
    access: {
        description: "Users can access their own tasks, or need Sys.Audit on /system/tasks.",
        permission: &Permission::Anybody,
    },
)]
/// Get task status.
async fn get_task_status(upid: UPID, rpcenv: &mut dyn RpcEnvironment) -> Result<Value, Error> {
    let auth_id: Authid = rpcenv.get_auth_id().unwrap().parse()?;
    check_task_access(&auth_id, &upid)?;

    let task_auth_id: Authid = upid.auth_id.parse()?;

    let mut result = json!({
        "upid": upid.to_string(),
        "node": upid.node,
        "pid": upid.pid,
        "pstart": upid.pstart,
        "starttime": upid.starttime,
        "type": upid.worker_type,
        "id": upid.worker_id,
        "user": task_auth_id.user(),
    });

    if task_auth_id.is_token() {
        result["tokenid"] = Value::from(task_auth_id.tokenname().unwrap().as_str());
    }

    if proxmox_rest_server::worker_is_active(&upid).await? {
        result["status"] = Value::from("running");
    } else {
        let exitstatus = upid_read_status(&upid).unwrap_or(TaskState::Unknown { endtime: 0 });
        result["status"] = Value::from("stopped");
        result["exitstatus"] = Value::from(exitstatus.to_string());
    };

    Ok(result)
}

const START_PARAM_SCHEMA: Schema =
    proxmox_schema::IntegerSchema::new("Start at this line when reading the tasklog")
        .minimum(0)
        .default(0)
        .schema();

const LIMIT_PARAM_SCHEMA: Schema = proxmox_schema::IntegerSchema::new(
    "The amount of lines to read from the tasklog. \
         Setting this parameter to 0 will return all lines until the end of the file.",
)
.minimum(0)
.default(50)
.schema();

const DOWNLOAD_PARAM_SCHEMA: Schema = proxmox_schema::BooleanSchema::new(
    "Whether the tasklog file should be downloaded. \
        This parameter can't be used in conjunction with other parameters",
)
.default(false)
.schema();

const TEST_STATUS_PARAM_SCHEMA: Schema = proxmox_schema::BooleanSchema::new(
    "Test task status, and set result attribute \"active\" accordingly.",
)
.schema();

#[sortable]
pub const API_METHOD_READ_TASK_LOG: proxmox_router::ApiMethod = proxmox_router::ApiMethod::new(
    &proxmox_router::ApiHandler::AsyncHttp(&read_task_log),
    &proxmox_schema::ObjectSchema::new(
        "Read the task log",
        &sorted!([
            ("node", false, &NODE_SCHEMA),
            ("upid", false, &UPID_SCHEMA),
            ("start", true, &START_PARAM_SCHEMA),
            ("limit", true, &LIMIT_PARAM_SCHEMA),
            ("download", true, &DOWNLOAD_PARAM_SCHEMA),
            ("test-status", true, &TEST_STATUS_PARAM_SCHEMA)
        ]),
    ),
)
.access(
    Some("Users can access their own tasks, or need Sys.Audit on /system/tasks."),
    &Permission::Anybody,
);
fn read_task_log(
    _parts: Parts,
    _req_body: Body,
    param: Value,
    _info: &proxmox_router::ApiMethod,
    rpcenv: Box<dyn RpcEnvironment>,
) -> proxmox_router::ApiResponseFuture {
    Box::pin(async move {
        use std::io::BufRead;

        let upid: UPID = param
            .as_object()
            .unwrap() // params are always objects
            .get("upid")
            .ok_or_else(|| format_err!("missing upid parameter"))?
            .as_str()
            .ok_or_else(|| format_err!("bad upid parameter type, expected a string"))?
            .parse()
            .map_err(|err| format_err!("invalid upid parameter - {err}"))?;
        let auth_id: Authid = rpcenv.get_auth_id().unwrap().parse()?;
        check_task_access(&auth_id, &upid)?;

        let download = param["download"].as_bool().unwrap_or(false);
        let path = upid_log_path(&upid)?;

        if download {
            if !param["start"].is_null()
                || !param["limit"].is_null()
                || !param["test-status"].is_null()
            {
                bail!("Parameter 'download' cannot be used with other parameters");
            }

            let header_disp = format!(
                "attachment; filename=task-{}-{}-{}.log",
                upid.node,
                upid.worker_type,
                proxmox_time::epoch_to_rfc3339_utc(upid.starttime)?
            );
            let stream = AsyncReaderStream::new(tokio::fs::File::open(path).await?);

            return Ok(Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/plain")
                .header(header::CONTENT_DISPOSITION, &header_disp)
                .body(Body::wrap_stream(stream))
                .unwrap());
        }
        let start = param["start"].as_u64().unwrap_or(0);
        let mut limit = param["limit"].as_u64().unwrap_or(50);
        let test_status = param["test-status"].as_bool().unwrap_or(false);

        let file = std::fs::File::open(path)?;

        let mut count: u64 = 0;
        let mut lines: Vec<Value> = vec![];
        let read_until_end = limit == 0;

        for line in std::io::BufReader::new(file).lines() {
            match line {
                Ok(line) => {
                    count += 1;
                    if count < start {
                        continue;
                    };
                    if !read_until_end {
                        if limit == 0 {
                            continue;
                        };
                        limit -= 1;
                    }

                    lines.push(json!({ "n": count, "t": line }));
                }
                Err(err) => {
                    log::error!("reading task log failed: {}", err);
                    break;
                }
            }
        }

        let mut json = json!({
            "data": lines,
            "total": count,
            "success": 1,
        });

        if test_status {
            let active = proxmox_rest_server::worker_is_active(&upid).await?;
            json["active"] = Value::from(active);
        }

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json.to_string()))
            .unwrap())
    })
}

fn tasktype(state: &TaskState) -> TaskStateType {
    match state {
        TaskState::OK { .. } => TaskStateType::OK,
        TaskState::Unknown { .. } => TaskStateType::Unknown,
        TaskState::Error { .. } => TaskStateType::Error,
        TaskState::Warning { .. } => TaskStateType::Warning,
    }
}

fn into_task_list_item(info: proxmox_rest_server::TaskListInfo) -> TaskListItem {
    let (endtime, status) = info.state.map_or_else(
        || (None, None),
        |a| (Some(a.endtime()), Some(a.to_string())),
    );

    TaskListItem {
        upid: info.upid_str,
        node: "localhost".to_string(),
        pid: info.upid.pid as i64,
        pstart: info.upid.pstart,
        starttime: info.upid.starttime,
        worker_type: info.upid.worker_type,
        worker_id: info.upid.worker_id,
        user: info.upid.auth_id,
        endtime,
        status,
    }
}
