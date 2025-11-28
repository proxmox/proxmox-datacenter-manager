use anyhow::Error;

use proxmox_router::{list_subdirs_api_method, Permission, Router, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use pdm_api_types::{remotes::REMOTE_ID_SCHEMA, NODE_SCHEMA, PRIV_RESOURCE_AUDIT};
use pve_api_types::StorageContent;

use crate::api::{nodes::sdn, pve::storage};

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("apt", &crate::api::remote_updates::APT_ROUTER),
    ("firewall", &super::firewall::NODE_FW_ROUTER),
    ("rrddata", &super::rrddata::NODE_RRD_ROUTER),
    ("network", &Router::new().get(&API_METHOD_GET_NETWORK)),
    (
        "termproxy",
        &Router::new().post(&crate::api::remote_shell::API_METHOD_SHELL_TICKET)
    ),
    (
        "vncwebsocket",
        &Router::new().upgrade(&crate::api::remote_shell::API_METHOD_SHELL_WEBSOCKET)
    ),
    ("sdn", &sdn::ROUTER),
    ("storage", &STORAGE_ROUTER),
    ("status", &Router::new().get(&API_METHOD_GET_STATUS)),
    (
        "subscription",
        &Router::new().get(&API_METHOD_GET_SUBSCRIPTION)
    ),
]);

const STORAGE_ROUTER: Router = Router::new()
    .get(&API_METHOD_GET_STORAGES)
    .match_all("storage", &storage::ROUTER);

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: { schema: NODE_SCHEMA },
            "interface-type": {
                type: pve_api_types::ListNetworksType,
                optional: true,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "node", "{node}"], PRIV_RESOURCE_AUDIT, false),
    },
    returns: {
        type: Array,
        description: "A list of network interfaces of a PVE remote.",
        items: {
            type: pve_api_types::NetworkInterface,
        }
    }
)]
/// Get network interfaces from PVE node
async fn get_network(
    remote: String,
    node: String,
    interface_type: Option<pve_api_types::ListNetworksType>,
) -> Result<Vec<pve_api_types::NetworkInterface>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let client = super::connect_to_remote(&remotes, &remote)?;
    let networks = client.list_networks(&node, interface_type).await?;
    Ok(networks)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: { schema: NODE_SCHEMA },
            content: {
                type: Array,
                description: "A list of contenttypes to filter for",
                items: {
                    type: StorageContent,
                },
                optional: true,
            },
            enabled: {
                type: bool,
                optional: true,
                description: "Only include enabled storages.",
            },
            format: {
                type: bool,
                optional: true,
                description: "Include format information.",
            },
            storage: {
                type: String,
                optional: true,
                description: "Only list status for specified storage.",
            },
            target: {
                type: String,
                optional: true,
                description: "If target is different to 'node', only list shared storages which are accessible by both.",
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "node", "{node}"], PRIV_RESOURCE_AUDIT, false),
        description: "if `target` is set, also requires PRIV_RESOURCE_AUDIT on /resource/{remote}/node/{target}"
    },
    returns: {
        type: Array,
        description: "A list of storages of a PVE remote.",
        items: {
            type: pve_api_types::StorageInfo
        }
    }
)]
/// Get status for all datastores
async fn get_storages(
    remote: String,
    node: String,
    content: Option<Vec<StorageContent>>,
    enabled: Option<bool>,
    format: Option<bool>,
    storage: Option<String>,
    target: Option<String>,
) -> Result<Vec<pve_api_types::StorageInfo>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let client = super::connect_to_remote(&remotes, &remote)?;
    let list = client
        .list_storages(&node, content, enabled, format, storage, target)
        .await?;
    Ok(list)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: { schema: NODE_SCHEMA },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "node", "{node}"], PRIV_RESOURCE_AUDIT, false),
    },
    returns: { type: pve_api_types::NodeStatus },
)]
/// Get status for the node
async fn get_status(remote: String, node: String) -> Result<pve_api_types::NodeStatus, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let client = super::connect_to_remote(&remotes, &remote)?;
    let result = client.node_status(&node).await?;
    Ok(result)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: { schema: NODE_SCHEMA },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "node", "{node}"], PRIV_RESOURCE_AUDIT, false),
    },
    returns: { type: pve_api_types::NodeStatus },
)]
/// Get subscription for the node
async fn get_subscription(
    remote: String,
    node: String,
) -> Result<pve_api_types::NodeSubscriptionInfo, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let client = super::connect_to_remote(&remotes, &remote)?;
    let result = client.get_subscription(&node).await?;
    Ok(result)
}
