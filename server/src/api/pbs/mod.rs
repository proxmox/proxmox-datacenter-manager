use anyhow::Error;
use serde::Deserialize;

use proxmox_router::http_bail;
use proxmox_router::ApiResponseFuture;
use proxmox_router::RpcEnvironment;
use proxmox_router::{list_subdirs_api_method, Permission, Router, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use pdm_api_types::remotes::REMOTE_ID_SCHEMA;
use pdm_api_types::PRIV_RESOURCE_AUDIT;

use crate::pbs_client;

pub const ROUTER: Router = Router::new().match_all("remote", &MAIN_ROUTER);

const MAIN_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([("datastore", &DATASTORE_ROUTER)]);

const DATASTORE_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_DATASTORES)
    .match_all("datastore", &DATASTORE_ITEM_ROUTER);

const DATASTORE_ITEM_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(DATASTORE_ITEM_SUBDIRS))
    .subdirs(DATASTORE_ITEM_SUBDIRS);

#[sortable]
const DATASTORE_ITEM_SUBDIRS: SubdirMap = &sorted!([(
    "snapshots",
    &Router::new().get(&API_METHOD_LIST_SNAPSHOTS_REAL)
),]);

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
        },
    },
    returns: {
        type: Array,
        description: "List of datastores configurations.",
        items: { type: pbs_api_types::DataStoreConfig },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// List the PBS remote's datastores.
async fn list_datastores(remote: String) -> Result<Vec<pbs_api_types::DataStoreConfig>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    Ok(pbs_client::connect_to_remote(&remotes, &remote)?
        .list_datastores()
        .await?)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            datastore: { schema: pbs_api_types::DATASTORE_SCHEMA },
            ns: {
                schema: pbs_api_types::BACKUP_NAMESPACE_SCHEMA,
                optional: true,
            },
        },
    },
    returns: pbs_api_types::ADMIN_DATASTORE_LIST_SNAPSHOTS_RETURN_TYPE,
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "datastore", "{datastore}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// List the PBS remote's datastores.
async fn list_snapshots(
    remote: String,
    datastore: String,
    ns: Option<String>,
) -> Result<Vec<pbs_api_types::SnapshotListItem>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    Ok(pbs_client::connect_to_remote(&remotes, &remote)?
        .list_snapshots(&datastore, ns.as_deref())
        .await?)
}

#[api(
    properties: {
        remote: { schema: REMOTE_ID_SCHEMA },
        datastore: { schema: pbs_api_types::DATASTORE_SCHEMA },
        ns: {
            schema: pbs_api_types::BACKUP_NAMESPACE_SCHEMA,
            optional: true,
        },
    },
)]
/// Parameters for the snapshot listing API method.
///
/// We use a "raw" handler for this to support proper streaming until the rest-server has a *real*
/// streaming method type (not the one we currently use).
///
/// The current `ApiMethod::Streaming*` should be renamed to `ApiMethod::Serializable*` and an
/// *actual* `ApiMethod::Streaming` added.
#[derive(Deserialize)]
struct ListSnapshotsParameters {
    remote: String,
    datastore: String,
    ns: Option<String>,
}

fn list_snapshots_raw(
    parts: http::request::Parts,
    mut req_body: hyper::Body,
    param: serde_json::Value,
    _info: &proxmox_router::ApiMethod,
    _rpcenv: Box<dyn RpcEnvironment>,
) -> ApiResponseFuture {
    use futures::stream::StreamExt;

    let param: ListSnapshotsParameters = match serde_json::from_value(param) {
        Ok(p) => p,
        Err(err) => return Box::pin(async move { Err(Error::from(err)) }),
    };

    Box::pin(async move {
        while let Some(chunk) = req_body.next().await {
            let chunk = chunk?;
            if !chunk.is_empty() {
                http_bail!(BAD_REQUEST, "unexpected request body");
            }
        }
        let snapshots = list_snapshots(param.remote, param.datastore, param.ns).await?;

        if parts
            .headers
            .get_all(http::header::ACCEPT)
            .iter()
            .any(|content| content == "application/json-seq")
        {
            // The client rqeuested streaming.
            // We now fake a "streaming" variant of this to test the UI's readable-stream.
            let (send, recv) = tokio::sync::mpsc::channel::<Result<Vec<u8>, Error>>(4);
            tokio::spawn(async move {
                for snapshot in snapshots {
                    let mut packet = Vec::with_capacity(256);
                    packet.push(0x1e);
                    if let Err(err) = serde_json::to_writer(&mut packet, &snapshot) {
                        log::error!("error serializing snapshot list item: {err}");
                        let _ = send.send(Err(err.into())).await;
                        return;
                    }
                    packet.push(b'\n');
                    if send.send(Ok(packet)).await.is_err() {
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            });

            let body = hyper::Body::wrap_stream(tokio_stream::wrappers::ReceiverStream::new(recv));
            Ok(http::Response::builder()
                .status(http::StatusCode::OK)
                .body(body)?)
        } else {
            let result = serde_json::to_vec(&snapshots)?;
            Ok(http::Response::builder()
                .status(http::StatusCode::OK)
                .body(hyper::Body::from(result))?)
        }
    })
}

const API_METHOD_LIST_SNAPSHOTS_REAL: proxmox_router::ApiMethod = proxmox_router::ApiMethod {
    handler: &proxmox_router::ApiHandler::AsyncHttp(&list_snapshots_raw),
    ..API_METHOD_LIST_SNAPSHOTS
};
