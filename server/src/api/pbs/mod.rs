use anyhow::Error;
use futures::StreamExt;

use proxmox_router::{list_subdirs_api_method, Permission, Router, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use pdm_api_types::remotes::{RemoteType, TlsProbeOutcome, REMOTE_ID_SCHEMA};
use pdm_api_types::{HOST_OPTIONAL_PORT_FORMAT, PRIV_RESOURCE_AUDIT, PRIV_SYS_MODIFY};

use crate::{
    connection::{self, probe_tls_connection},
    pbs_client::{self, get_remote},
};

mod rrddata;

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("remotes", &REMOTES_ROUTER),
    ("probe-tls", &Router::new().post(&API_METHOD_PROBE_TLS)),
]);

const REMOTES_ROUTER: Router = Router::new().match_all("remote", &MAIN_ROUTER);

pub const MAIN_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(REMOTE_SUBDIRS))
    .subdirs(REMOTE_SUBDIRS);

#[sortable]
const REMOTE_SUBDIRS: SubdirMap = &sorted!([
    ("rrddata", &rrddata::PBS_NODE_RRD_ROUTER),
    ("datastore", &DATASTORE_ROUTER)
]);

const DATASTORE_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_DATASTORES)
    .match_all("datastore", &DATASTORE_ITEM_ROUTER);

const DATASTORE_ITEM_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(DATASTORE_ITEM_SUBDIRS))
    .subdirs(DATASTORE_ITEM_SUBDIRS);

#[sortable]
const DATASTORE_ITEM_SUBDIRS: SubdirMap = &sorted!([
    ("rrddata", &rrddata::PBS_DATASTORE_RRD_ROUTER),
    (
        "snapshots",
        &Router::new().get(&API_METHOD_LIST_SNAPSHOTS_2)
    ),
]);

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
    stream: true,
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
async fn list_snapshots_2(
    remote: String,
    datastore: String,
    ns: Option<String>,
) -> Result<proxmox_router::Stream, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    Ok(async_stream::try_stream! {
        let remote = get_remote(&remotes, &remote)?;
        let mut snapshots = connection::make_pbs_client(remote)?
            .list_snapshots(&datastore, ns.as_deref())
            .await?;
        while let Some(elem) = snapshots.next().await {
            if let Err(err) = &elem {
                log::error!("got an error in a record: {err:?}");
            }
            yield elem?.into();
        }
    }
    .into())
}

#[api(
    input: {
        properties: {
            hostname: {
                type: String,
                format: &HOST_OPTIONAL_PORT_FORMAT,
                description: "Hostname (with optional port) of the target remote",
            },
            fingerprint: {
                type: String,
                description: "Fingerprint of the target remote.",
                optional: true,
            },
        },
    },
    access: {
        permission:
            &Permission::Privilege(&["/"], PRIV_SYS_MODIFY, false),
    },
)]
/// Probe the hosts TLS certificate.
///
/// If the certificate is not trusted with the given parameters, returns the certificate
/// information.
async fn probe_tls(
    hostname: String,
    fingerprint: Option<String>,
) -> Result<TlsProbeOutcome, Error> {
    probe_tls_connection(RemoteType::Pbs, hostname, fingerprint).await
}
