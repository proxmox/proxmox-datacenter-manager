//! Manage PVE instances.

use std::sync::Arc;

use anyhow::{bail, format_err, Error};

use proxmox_access_control::CachedUserInfo;
use proxmox_router::{
    http_bail, http_err, list_subdirs_api_method, Permission, Router, RpcEnvironment, SubdirMap,
};
use proxmox_schema::api;
use proxmox_schema::property_string::PropertyString;
use proxmox_section_config::typed::SectionConfigData;
use proxmox_sortable_macro::sortable;

use pdm_api_types::remotes::{NodeUrl, Remote, RemoteType, REMOTE_ID_SCHEMA};
use pdm_api_types::resource::PveResource;
use pdm_api_types::{
    Authid, RemoteUpid, HOST_OPTIONAL_PORT_FORMAT, PRIV_RESOURCE_AUDIT, PRIV_RESOURCE_DELETE,
    PRIV_SYS_MODIFY,
};

use pve_api_types::client::PveClient;
use pve_api_types::{ClusterResourceKind, ClusterResourceType, ListRealm, PveUpid};

use super::resources::{map_pve_lxc, map_pve_node, map_pve_qemu, map_pve_storage};

use crate::{connection, task_cache};

mod lxc;
mod node;
mod qemu;
mod rrddata;
pub mod tasks;

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("remotes", &REMOTES_ROUTER),
    ("scan", &Router::new().post(&API_METHOD_SCAN_REMOTE_PVE)),
    (
        "realms",
        &Router::new().post(&API_METHOD_LIST_REALM_REMOTE_PVE)
    )
]);

pub const REMOTES_ROUTER: Router = Router::new().match_all("remote", &MAIN_ROUTER);

const MAIN_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(REMOTE_SUBDIRS))
    .subdirs(REMOTE_SUBDIRS);

#[sortable]
const REMOTE_SUBDIRS: SubdirMap = &sorted!([
    ("lxc", &lxc::ROUTER),
    ("nodes", &NODES_ROUTER),
    ("qemu", &qemu::ROUTER),
    ("resources", &RESOURCES_ROUTER),
    ("tasks", &tasks::ROUTER),
]);

const NODES_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_NODES)
    .match_all("node", &node::ROUTER);

const RESOURCES_ROUTER: Router = Router::new().get(&API_METHOD_CLUSTER_RESOURCES);

// converts a remote + PveUpid into a RemoteUpid and starts tracking it
fn new_remote_upid(remote: String, upid: PveUpid) -> Result<RemoteUpid, Error> {
    let remote_upid: RemoteUpid = (remote, upid.to_string()).try_into()?;
    task_cache::track_running_task(remote_upid.clone());
    Ok(remote_upid)
}

pub(crate) fn get_remote<'a>(
    config: &'a SectionConfigData<Remote>,
    id: &str,
) -> Result<&'a Remote, Error> {
    let remote = super::remotes::get_remote(config, id)?;
    if remote.ty != RemoteType::Pve {
        bail!("remote {id:?} is not a pve remote");
    }
    Ok(remote)
}

pub async fn connect_or_login(remote: &Remote) -> Result<Box<dyn PveClient + Send + Sync>, Error> {
    connection::make_pve_client_and_login(remote).await
}

pub fn connect(remote: &Remote) -> Result<Box<dyn PveClient + Send + Sync>, Error> {
    connection::make_pve_client(remote)
}

fn connect_to_remote(
    config: &SectionConfigData<Remote>,
    id: &str,
) -> Result<Box<dyn PveClient + Send + Sync>, Error> {
    connect(get_remote(config, id)?)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
        },
    },
    returns: {
        type: Array,
        description: "List of basic PVE node information",
        items: { type: pve_api_types::ClusterNodeIndexResponse },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Query the remote's version.
///
/// FIXME: Should we add an option to explicitly query the entire cluster to get a full version
/// overview?
pub async fn list_nodes(
    remote: String,
) -> Result<Vec<pve_api_types::ClusterNodeIndexResponse>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    Ok(connect_to_remote(&remotes, &remote)?.list_nodes().await?)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            kind: {
                type: ClusterResourceKind,
                optional: true,
            },
        },
    },
    returns: {
        type: Array,
        description: "List all the resources in a PVE cluster.",
        items: { type: PveResource },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Query the cluster's resources.
///
// FIXME: Use more fine grained permissions and filter on:
//   - `/resource/{remote-id}/{resource-type=guest,storage}/{resource-id}`
pub async fn cluster_resources(
    remote: String,
    kind: Option<ClusterResourceKind>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<PveResource>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let user_info = CachedUserInfo::new()?;
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .ok_or_else(|| format_err!("no authid available"))?
        .parse()?;
    if !user_info.any_privs_below(&auth_id, &["resource", &remote], PRIV_RESOURCE_AUDIT)? {
        http_bail!(UNAUTHORIZED, "user has no access to resource list");
    }

    let cluster_resources = connect_to_remote(&remotes, &remote)?
        .cluster_resources(kind)
        .await?
        .into_iter()
        .filter_map(|r| map_pve_resource(&remote, r));

    Ok(cluster_resources.collect())
}

fn map_pve_resource(remote: &str, resource: pve_api_types::ClusterResource) -> Option<PveResource> {
    match resource.ty {
        ClusterResourceType::Node => map_pve_node(remote, resource).map(PveResource::Node),
        ClusterResourceType::Lxc => map_pve_lxc(remote, resource).map(PveResource::Lxc),
        ClusterResourceType::Qemu => map_pve_qemu(remote, resource).map(PveResource::Qemu),
        ClusterResourceType::Storage => map_pve_storage(remote, resource).map(PveResource::Storage),
        _ => None,
    }
}

/// Common permission checks between listing qemu & lxc guests.
///
/// Returns the data commonly reused afterwards: (auth_id, CachedUserInfo, top_level_allowed).
fn check_guest_list_permissions(
    remote: &str,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<(Authid, Arc<CachedUserInfo>, bool), Error> {
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .ok_or_else(|| format_err!("no authid available"))?
        .parse()?;

    let user_info = CachedUserInfo::new()?;

    if !user_info.any_privs_below(&auth_id, &["resource", remote], PRIV_RESOURCE_AUDIT)? {
        http_bail!(UNAUTHORIZED, "user has no access to resource list");
    }

    let top_level_allowed =
        0 != PRIV_RESOURCE_AUDIT & user_info.lookup_privs(&auth_id, &["resource", remote]);

    Ok((auth_id, user_info, top_level_allowed))
}

/// Shared permission check for a specific guest.
fn check_guest_permissions(
    auth_id: &Authid,
    user_info: &CachedUserInfo,
    remote: &str,
    privilege: u64,
    vmid: u32,
) -> bool {
    let auth_privs =
        user_info.lookup_privs(auth_id, &["resource", remote, "guest", &vmid.to_string()]);
    auth_privs & privilege != 0
}

async fn find_node_for_vm(
    node: Option<String>,
    vmid: u32,
    pve: &(dyn PveClient + Send + Sync),
) -> Result<String, Error> {
    // FIXME: The pve client should cache the resources
    Ok(match node {
        Some(node) => node,
        None => pve
            .cluster_resources(Some(ClusterResourceKind::Vm))
            .await?
            .into_iter()
            .find(|entry| entry.vmid == Some(vmid))
            .and_then(|entry| entry.node)
            .ok_or_else(|| http_err!(NOT_FOUND, "no such vmid"))?,
    })
}

fn check_guest_delete_perms(
    rpcenv: &mut dyn RpcEnvironment,
    remote: &str,
    vmid: u32,
) -> Result<(), Error> {
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .ok_or_else(|| format_err!("no authid available"))?
        .parse()?;

    CachedUserInfo::new()?.check_privs(
        &auth_id,
        &["resource", remote, "guest", &vmid.to_string()],
        PRIV_RESOURCE_DELETE,
        false,
    )
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
/// Scans the given connection info for pve cluster information
pub async fn scan_remote_pve(
    hostname: String,
    fingerprint: Option<String>,
    authid: Authid,
    token: String,
) -> Result<Remote, Error> {
    let mut remote = Remote {
        ty: RemoteType::Pve,
        id: String::new(),
        nodes: vec![PropertyString::new(NodeUrl {
            hostname,
            fingerprint,
        })],
        authid: authid.clone(),
        token,
    };

    let client = connect_or_login(&remote)
        .await
        .map_err(|err| format_err!("could not login: {err}"))?;

    let nodes: Vec<_> = client
        .list_nodes()
        .await?
        .into_iter()
        .map(|node| {
            let url = NodeUrl {
                hostname: node.node,
                fingerprint: node.ssl_fingerprint,
            };
            PropertyString::new(url)
        })
        .collect();

    if nodes.is_empty() {
        bail!("no node list returned");
    }

    remote.nodes = nodes;

    if let Ok(info) = client.cluster_config_join(None).await {
        if let Some(Some(name)) = info.totem.get("cluster_name").map(|name| name.as_str()) {
            remote.id = name.to_string();
        }
    }

    if remote.id.is_empty() {
        // we did not get a cluster name, so fall back to the first nodename
        remote.id = remote
            .nodes
            .first()
            .map(|node| node.hostname.clone())
            .unwrap_or_default();
    }

    Ok(remote)
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
/// Scans the given connection info for pve cluster information
pub async fn list_realm_remote_pve(
    hostname: String,
    fingerprint: Option<String>,
) -> Result<Vec<ListRealm>, Error> {
    // dummy remote to connect
    let remote = Remote {
        ty: RemoteType::Pve,
        id: String::new(),
        nodes: vec![PropertyString::new(NodeUrl {
            hostname,
            fingerprint,
        })],
        authid: "root@pam".parse()?,
        token: String::new(),
    };

    let client = connection::make_pve_client(&remote)?;
    let list = client.list_domains().await?;

    Ok(list)
}
