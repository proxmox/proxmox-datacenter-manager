use anyhow::{Error, format_err};
use futures::StreamExt;

use proxmox_router::{Permission, Router, SubdirMap, list_subdirs_api_method};
use proxmox_schema::api;
use proxmox_schema::property_string::PropertyString;
use proxmox_sortable_macro::sortable;

use pdm_api_types::remotes::{
    NodeUrl, REMOTE_ID_SCHEMA, Remote, RemoteListEntry, RemoteType, TlsProbeOutcome,
};
use pdm_api_types::{
    Authid, HOST_OPTIONAL_PORT_FORMAT, PRIV_RESOURCE_AUDIT, PRIV_SYS_MODIFY, RemoteUpid,
};

use crate::{
    connection::{self, probe_tls_connection},
    pbs_client::{self, PbsClient, get_remote},
};

use crate::remote_tasks;

mod node;
mod rrddata;
pub mod tasks;

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("remotes", &REMOTES_ROUTER),
    ("scan", &Router::new().post(&API_METHOD_SCAN_REMOTE_PBS)),
    ("probe-tls", &Router::new().post(&API_METHOD_PROBE_TLS)),
    (
        "realms",
        &Router::new().get(&API_METHOD_LIST_REALM_REMOTE_PBS)
    )
]);

const REMOTES_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_REMOTES)
    .match_all("remote", &MAIN_ROUTER);

pub const MAIN_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(REMOTE_SUBDIRS))
    .subdirs(REMOTE_SUBDIRS);

const NODES_ROUTER: Router = Router::new().match_all("node", &node::ROUTER);

#[sortable]
const REMOTE_SUBDIRS: SubdirMap = &sorted!([
    ("nodes", &NODES_ROUTER),
    ("status", &Router::new().get(&API_METHOD_GET_STATUS)),
    ("rrddata", &rrddata::PBS_NODE_RRD_ROUTER),
    ("datastore", &DATASTORE_ROUTER),
    ("tasks", &tasks::ROUTER),
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
        "namespaces",
        &Router::new().get(&API_METHOD_LIST_NAMESPACES)
    ),
    ("snapshots", &Router::new().get(&API_METHOD_LIST_SNAPSHOTS)),
]);

// converts a remote + pbs_api_types::UPID into a RemoteUpid and starts tracking it
pub async fn new_remote_upid(
    remote: String,
    upid: pbs_api_types::UPID,
) -> Result<RemoteUpid, Error> {
    let remote_upid = remote_tasks::track_running_pbs_task(remote, upid).await?;
    Ok(remote_upid)
}

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
    Ok(super::remotes::RemoteIterator::new()?
        .remote_type(RemoteType::Pbs)
        .into_names()
        .map(|name| RemoteListEntry { remote: name })
        .collect())
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
    Ok(pbs_client::connect_to_remote_by_id(&remote)?
        .list_datastores()
        .await?)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            params: {
                type: pbs_client::DatstoreListNamespaces,
                flatten: true,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "datastore", "{datastore}"], PRIV_RESOURCE_AUDIT, false),
    },
    returns: {
        type: Array,
        description: "A list of namespaces of a PBS remote's datastore.",
        items: { type: pbs_api_types::NamespaceListItem }
    }
)]
/// List the namespaces of PBS remote's datastore.
async fn list_namespaces(
    remote: String,
    params: pbs_client::DatstoreListNamespaces,
) -> Result<Vec<pbs_api_types::NamespaceListItem>, Error> {
    Ok(pbs_client::connect_to_remote_by_id(&remote)?
        .list_datastore_namespaces(params)
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
async fn list_snapshots(
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
    let endpoint = hostname.clone();
    connection::run_remote_operation(&endpoint, async move {
        probe_tls_connection(RemoteType::Pbs, hostname, fingerprint).await
    })
    .await
}

pub async fn connect_or_login(
    remote: &Remote,
) -> Result<Box<PbsClient<proxmox_client::Client>>, Error> {
    connection::make_pbs_client_and_login(remote).await
}

async fn validate_credentials<C>(client: &PbsClient<C>) -> Result<(), proxmox_client::Error>
where
    C: proxmox_client::HttpApiClient<Body = proxmox_http::Body>,
{
    client.version().await?;
    Ok(())
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
    returns: { type: Remote }
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
    let endpoint = hostname.clone();
    connection::run_remote_operation(&endpoint, async move {
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

        let client = connect_or_login(&remote)
            .await
            .map_err(|err| format_err!("could not login: {err}"))?;
        if remote.authid.is_token() {
            validate_credentials(&client)
                .await
                .map_err(|err| format_err!("could not login: {err}"))?;
        }

        Ok(remote)
    })
    .await
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
    returns: {
        type: Array,
        description: "A list of realms of a PBS remote.",
        items: {
            type: pbs_api_types::BasicRealmInfo,
        }
    },
)]
/// List available authentication realms of a PBS remote.
pub async fn list_realm_remote_pbs(
    hostname: String,
    fingerprint: Option<String>,
) -> Result<Vec<pbs_api_types::BasicRealmInfo>, Error> {
    let endpoint = hostname.clone();
    connection::run_remote_operation(&endpoint, async move {
        // dummy remote to connect
        let remote = Remote {
            ty: RemoteType::Pbs,
            id: String::new(),
            nodes: vec![PropertyString::new(NodeUrl {
                hostname,
                fingerprint,
            })],
            authid: "root@pam".parse()?,
            token: String::new(),
            web_url: None,
        };

        let client = connection::make_onboarding_pbs_client(&remote)?;
        let list = client.list_domains().await?;

        Ok(list)
    })
    .await
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
        description: "The user needs to have at least the `Resource.Audit` privilege on `/resource/{remote}`."
    },
    returns: { type: pbs_api_types::NodeStatus }
)]
/// Get status for the PBS remote
async fn get_status(remote: String) -> Result<pbs_api_types::NodeStatus, Error> {
    Ok(pbs_client::connect_to_remote_by_id(&remote)?
        .node_status()
        .await?)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use futures::future::{Pending, Ready, pending, ready};
    use http::{Method, StatusCode};
    use proxmox_client::{
        Error as ClientError, HttpApiClient, HttpApiResponse, HttpApiResponseStream,
    };
    use serde::Serialize;

    use crate::pbs_client::PbsClient;

    use super::validate_credentials;

    #[derive(Default)]
    struct UnauthorizedClient {
        requests: RefCell<Vec<(Method, String)>>,
    }

    impl HttpApiClient for UnauthorizedClient {
        type ResponseFuture<'a>
            = Ready<Result<HttpApiResponse, ClientError>>
        where
            Self: 'a;
        type Body = proxmox_http::Body;
        type ResponseStreamFuture<'a>
            = Pending<Result<HttpApiResponseStream<Self::Body>, ClientError>>
        where
            Self: 'a;

        fn request<'a, T>(
            &'a self,
            method: Method,
            path_and_query: &'a str,
            _params: Option<T>,
        ) -> Self::ResponseFuture<'a>
        where
            T: Serialize + 'a,
        {
            self.requests
                .borrow_mut()
                .push((method, path_and_query.to_string()));
            ready(Err(ClientError::Api(
                StatusCode::UNAUTHORIZED,
                "authentication failed".to_string(),
            )))
        }

        fn streaming_request<'a, T>(
            &'a self,
            _method: Method,
            _path_and_query: &'a str,
            _params: Option<T>,
        ) -> Self::ResponseStreamFuture<'a>
        where
            T: Serialize + 'a,
        {
            pending()
        }
    }

    #[tokio::test]
    async fn credential_validation_rejects_failed_api_request() {
        let client = PbsClient(UnauthorizedClient::default());

        let error = validate_credentials(&client).await.unwrap_err();

        assert!(matches!(
            error,
            ClientError::Api(StatusCode::UNAUTHORIZED, _)
        ));
        assert_eq!(
            client.0.requests.into_inner(),
            [(Method::GET, "/api2/extjs/version".to_string())]
        );
    }
}
