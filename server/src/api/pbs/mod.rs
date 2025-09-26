use anyhow::{format_err, Error};
use futures::StreamExt;

use proxmox_router::{list_subdirs_api_method, Permission, Router, SubdirMap};
use proxmox_schema::api;
use proxmox_schema::property_string::PropertyString;
use proxmox_sortable_macro::sortable;

use pdm_api_types::remotes::{
    NodeUrl, Remote, RemoteListEntry, RemoteType, TlsProbeOutcome, REMOTE_ID_SCHEMA,
};
use pdm_api_types::{Authid, HOST_OPTIONAL_PORT_FORMAT, PRIV_RESOURCE_AUDIT, PRIV_SYS_MODIFY};

use crate::{
    connection::{self, probe_tls_connection},
    pbs_client::{self, get_remote, PbsClient},
};

mod rrddata;

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("remotes", &REMOTES_ROUTER),
    ("scan", &Router::new().post(&API_METHOD_SCAN_REMOTE_PBS)),
    ("probe-tls", &Router::new().post(&API_METHOD_PROBE_TLS)),
]);

const REMOTES_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_REMOTES)
    .match_all("remote", &MAIN_ROUTER);

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
    returns: {
        type: Array,
        description: "List of PBS remotes",
        items: {
            type: pdm_api_types::remotes::RemoteListEntry,
        },
    },
)]
/// Return the list of PBS remotes
fn list_remotes() -> Result<Vec<RemoteListEntry>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let remotes = remotes
        .into_iter()
        .filter_map(|(remote, Remote { ty, .. })| match ty {
            RemoteType::Pbs => Some(RemoteListEntry { remote }),
            RemoteType::Pve => None,
        })
        .collect();
    Ok(remotes)
}

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

pub async fn connect_or_login(remote: &Remote) -> Result<Box<PbsClient>, Error> {
    connection::make_pbs_client_and_login(remote).await
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
            "authid": {
                type: Authid,
            },
            "token": {
                type: String,
                description: "The token secret or the user password.",
            },
        },
    },
    access: {
        permission:
            &Permission::Privilege(&["/"], PRIV_SYS_MODIFY, false),
    },
)]
/// Scans the given connection info for pbs host information.
///
/// Checks login using the provided credentials.
pub async fn scan_remote_pbs(
    hostname: String,
    fingerprint: Option<String>,
    authid: Authid,
    token: String,
) -> Result<Remote, Error> {
    let remote = Remote {
        ty: RemoteType::Pbs,
        id: hostname.clone(),
        nodes: vec![PropertyString::new(NodeUrl {
            hostname,
            fingerprint,
        })],
        authid: authid.clone(),
        token,
        web_url: None,
    };

    let _client = connect_or_login(&remote)
        .await
        .map_err(|err| format_err!("could not login: {err}"))?;

    Ok(remote)
}
