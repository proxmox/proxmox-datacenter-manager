use std::process::Stdio;

use futures::FutureExt;
use http::request::Parts;
use http::{Response, StatusCode, header};
use serde_json::Value;
use tokio::process::Command;

use proxmox_async::stream::AsyncReaderStream;
use proxmox_http::Body;
use proxmox_router::{
    ApiHandler, ApiMethod, ApiResponseFuture, Permission, Router, RpcEnvironment,
};
use proxmox_schema::{AllOfSchema, ApiType, ObjectSchema, ParameterSchema, Schema};

use proxmox_syslog_api::JournalFilter;

use pdm_api_types::{NODE_SCHEMA, PRIV_SYS_AUDIT};

const NODE_OBJECT_SCHEMA: Schema =
    ObjectSchema::new("Node parameter.", &[("node", false, &NODE_SCHEMA)]).schema();

pub const API_METHOD_GET_JOURNAL: ApiMethod = ApiMethod::new_full(
    &ApiHandler::AsyncHttp(&get_journal),
    ParameterSchema::AllOf(&AllOfSchema::new(
        "Read syslog entries.",
        &[&NODE_OBJECT_SCHEMA, &<JournalFilter as ApiType>::API_SCHEMA],
    )),
)
.protected(true)
.access(
    None,
    &Permission::Privilege(&["system", "log"], PRIV_SYS_AUDIT, false),
);

/// Read syslog entries.
fn get_journal(
    _parts: Parts,
    _req_body: hyper::body::Incoming,
    param: Value,
    _info: &ApiMethod,
    _rpcenv: Box<dyn RpcEnvironment>,
) -> ApiResponseFuture {
    async move {
        let filter: JournalFilter = serde_json::from_value(param)?;
        let args = proxmox_syslog_api::journal_args(&filter);

        let mut child = Command::new("mini-journalreader")
            .args(&args)
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let stdout = child.stdout.take().expect("piped stdout is available");

        // reap the short-lived child, logging a failure so a broken reader is not silently empty
        tokio::spawn(async move {
            match child.wait().await {
                Ok(status) if !status.success() => {
                    log::error!("mini-journalreader exited with {status}");
                }
                Err(err) => log::error!("waiting for mini-journalreader failed: {err}"),
                _ => {}
            }
        });

        let stream = AsyncReaderStream::new(stdout);

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::wrap_stream(stream))
            .unwrap())
    }
    .boxed()
}

pub const ROUTER: Router = Router::new().get(&API_METHOD_GET_JOURNAL);
