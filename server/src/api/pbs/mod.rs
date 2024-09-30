use anyhow::Error;
use futures::StreamExt;

use proxmox_router::{list_subdirs_api_method, Permission, Router, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use pdm_api_types::remotes::REMOTE_ID_SCHEMA;
use pdm_api_types::PRIV_RESOURCE_AUDIT;

use crate::pbs_client;

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([("remotes", &REMOTES_ROUTER)]);

const REMOTES_ROUTER: Router = Router::new().match_all("remote", &MAIN_ROUTER);

pub const MAIN_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(REMOTE_SUBDIRS))
    .subdirs(REMOTE_SUBDIRS);

#[sortable]
const REMOTE_SUBDIRS: SubdirMap = &sorted!([("datastore", &DATASTORE_ROUTER)]);

const DATASTORE_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_DATASTORES)
    .match_all("datastore", &DATASTORE_ITEM_ROUTER);

const DATASTORE_ITEM_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(DATASTORE_ITEM_SUBDIRS))
    .subdirs(DATASTORE_ITEM_SUBDIRS);

#[sortable]
const DATASTORE_ITEM_SUBDIRS: SubdirMap = &sorted!([(
    "snapshots",
    &Router::new().get(&API_METHOD_LIST_SNAPSHOTS_2)
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
        let mut snapshots = pbs_client::connect_to_remote(&remotes, &remote)?
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
