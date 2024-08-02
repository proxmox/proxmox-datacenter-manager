//! Manage PVE instances.

use std::sync::Arc;

use anyhow::{bail, format_err, Error};

use proxmox_access_control::CachedUserInfo;
use proxmox_client::{Client, TlsOptions};
use proxmox_router::{
    http_bail, http_err, list_subdirs_api_method, Permission, Router, RpcEnvironment, SubdirMap,
};
use proxmox_schema::api;
use proxmox_section_config_typed::SectionConfigData;
use proxmox_sortable_macro::sortable;

use pdm_api_types::remotes::{Remote, RemoteType, REMOTE_ID_SCHEMA};
use pdm_api_types::{
    Authid, ConfigurationState, RemoteUpid, NODE_SCHEMA, PRIV_RESOURCE_AUDIT, PRIV_RESOURCE_DELETE,
    PRIV_RESOURCE_MANAGE, PRIV_RESOURCE_MIGRATE, SNAPSHOT_NAME_SCHEMA, VMID_SCHEMA,
};
use pve_api_types::client::PveClient;
use pve_api_types::ClusterResourceKind;

pub mod tasks;

pub const ROUTER: Router = Router::new().match_all("remote", &MAIN_ROUTER);

const MAIN_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("lxc", &LXC_ROUTER),
    ("nodes", &NODES_ROUTER),
    ("qemu", &QEMU_ROUTER),
    ("resources", &RESOURCES_ROUTER),
    ("tasks", &tasks::ROUTER),
]);

const LXC_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_LXC)
    .match_all("vmid", &LXC_VM_ROUTER);

const LXC_VM_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(LXC_VM_SUBDIRS))
    .subdirs(LXC_VM_SUBDIRS);
#[sortable]
const LXC_VM_SUBDIRS: SubdirMap = &sorted!([
    ("config", &Router::new().get(&API_METHOD_LXC_GET_CONFIG)),
    ("start", &Router::new().post(&API_METHOD_LXC_START)),
    ("stop", &Router::new().post(&API_METHOD_LXC_STOP)),
    ("shutdown", &Router::new().post(&API_METHOD_LXC_SHUTDOWN)),
    (
        "remote-migrate",
        &Router::new().post(&API_METHOD_LXC_REMOTE_MIGRATE)
    ),
]);

const NODES_ROUTER: Router = Router::new().get(&API_METHOD_LIST_NODES);

const QEMU_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_QEMU)
    .match_all("vmid", &QEMU_VM_ROUTER);

const QEMU_VM_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(QEMU_VM_SUBDIRS))
    .subdirs(QEMU_VM_SUBDIRS);
#[sortable]
const QEMU_VM_SUBDIRS: SubdirMap = &sorted!([
    ("config", &Router::new().get(&API_METHOD_QEMU_GET_CONFIG)),
    ("start", &Router::new().post(&API_METHOD_QEMU_START)),
    ("stop", &Router::new().post(&API_METHOD_QEMU_STOP)),
    ("shutdown", &Router::new().post(&API_METHOD_QEMU_SHUTDOWN)),
    (
        "remote-migrate",
        &Router::new().post(&API_METHOD_QEMU_REMOTE_MIGRATE)
    ),
]);

const RESOURCES_ROUTER: Router = Router::new().get(&API_METHOD_CLUSTER_RESOURCES);

pub fn get_remote<'a>(
    config: &'a SectionConfigData<Remote>,
    id: &str,
) -> Result<&'a Remote, Error> {
    let remote = super::remotes::get_remote(config, id)?;
    if remote.ty != RemoteType::Pve {
        bail!("remote {id:?} is not a pve remote");
    }
    Ok(remote)
}

pub fn connect(remote: &Remote) -> Result<PveClient<Client>, Error> {
    let node = remote
        .nodes
        .first()
        .ok_or_else(|| format_err!("no nodes configured for remote"))?;

    let mut options = TlsOptions::default();
    if let Some(fp) = &node.fingerprint {
        options = TlsOptions::parse_fingerprint(fp)?;
    }

    let uri = format!("https://{}:8006", node.hostname).parse()?;
    let client = Client::with_options(uri, options, Default::default())?;

    client.set_authentication(proxmox_client::Token {
        userid: remote.userid.clone(),
        prefix: "PVEAPIToken".to_string(),
        value: remote.token.to_string(),
    });

    Ok(PveClient(client))
}

pub fn connect_to_remote(
    config: &SectionConfigData<Remote>,
    id: &str,
) -> Result<PveClient<Client>, Error> {
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
        items: { type: pve_api_types::ClusterResource },
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
) -> Result<Vec<pve_api_types::ClusterResource>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    Ok(connect_to_remote(&remotes, &remote)?
        .cluster_resources(kind)
        .await?)
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

    let top_level_allowed = 0 != user_info.lookup_privs(&auth_id, &["resource", remote]);

    Ok((auth_id, user_info, top_level_allowed))
}

/// Shared permission check for a specific guest.
fn check_guest_permissions(
    auth_id: &Authid,
    user_info: &CachedUserInfo,
    remote: &str,
    vmid: u32,
) -> bool {
    0 != user_info.lookup_privs(auth_id, &["resource", remote, "guest", &vmid.to_string()])
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
        },
    },
    returns: {
        type: Array,
        description: "Get a list of VMs",
        items: { type: pve_api_types::VmEntry },
    },
    access: {
        permission: &Permission::Anybody,
        description: "Returns the resources the user has access to.",
    },
)]
/// Query the remote's list of qemu VMs. If no node is provided, the all nodes are queried.
pub async fn list_qemu(
    remote: String,
    node: Option<String>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<pve_api_types::VmEntry>, Error> {
    let (auth_id, user_info, top_level_allowed) = check_guest_list_permissions(&remote, rpcenv)?;

    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

    let list = if let Some(node) = node {
        pve.list_qemu(&node, None).await?
    } else {
        let mut list = Vec::new();
        for node in pve.list_nodes().await? {
            list.extend(pve.list_qemu(&node.node, None).await?);
        }
        list
    };

    if top_level_allowed {
        return Ok(list);
    }

    Ok(list
        .into_iter()
        .filter(|entry| check_guest_permissions(&auth_id, &user_info, &remote, entry.vmid))
        .collect())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
        },
    },
    returns: {
        type: Array,
        description: "Get a list of containers.",
        items: { type: pve_api_types::VmEntry },
    },
)]
/// Query the remote's list of lxc containers. If no node is provided, the all nodes are queried.
pub async fn list_lxc(
    remote: String,
    node: Option<String>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<pve_api_types::LxcEntry>, Error> {
    let (auth_id, user_info, top_level_allowed) = check_guest_list_permissions(&remote, rpcenv)?;

    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

    let list = if let Some(node) = node {
        pve.list_lxc(&node).await?
    } else {
        let mut list = Vec::new();
        for node in pve.list_nodes().await? {
            list.extend(pve.list_lxc(&node.node).await?);
        }
        list
    };

    if top_level_allowed {
        return Ok(list);
    }

    Ok(list
        .into_iter()
        .filter(|entry| check_guest_permissions(&auth_id, &user_info, &remote, entry.vmid))
        .collect())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA },
            state: { type: ConfigurationState },
            snapshot: {
                schema: SNAPSHOT_NAME_SCHEMA,
                optional: true,
            },
        },
    },
    returns: { type: pve_api_types::QemuConfig },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get the configuration of a qemu VM from a remote. If a node is provided, the VM must be on that
/// node, otherwise the node is determined automatically.
pub async fn qemu_get_config(
    remote: String,
    node: Option<String>,
    vmid: u32,
    state: ConfigurationState,
    snapshot: Option<String>,
) -> Result<pve_api_types::QemuConfig, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

    // FIXME: The pve client should cache the resources and provide
    let node = match node {
        Some(node) => node,
        None => pve
            .cluster_resources(Some(ClusterResourceKind::Vm))
            .await?
            .into_iter()
            .find(|entry| entry.vmid == Some(vmid))
            .and_then(|entry| entry.node)
            .ok_or_else(|| http_err!(NOT_FOUND, "no such vmid"))?,
    };

    Ok(pve
        .qemu_get_config(&node, vmid, state.current(), snapshot)
        .await?)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA },
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Start a remote qemu vm.
pub async fn qemu_start(
    remote: String,
    node: Option<String>,
    vmid: u32,
) -> Result<RemoteUpid, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

    // FIXME: The pve client should cache the resources and provide
    let node = match node {
        Some(node) => node,
        None => pve
            .cluster_resources(Some(ClusterResourceKind::Vm))
            .await?
            .into_iter()
            .find(|entry| entry.vmid == Some(vmid))
            .and_then(|entry| entry.node)
            .ok_or_else(|| http_err!(NOT_FOUND, "no such vmid"))?,
    };

    let upid = pve
        .start_qemu_async(&node, vmid, Default::default())
        .await?;

    (remote, upid.to_string()).try_into()
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA },
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Stop a remote qemu vm.
pub async fn qemu_stop(
    remote: String,
    node: Option<String>,
    vmid: u32,
) -> Result<RemoteUpid, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

    // FIXME: The pve client should cache the resources and provide
    let node = match node {
        Some(node) => node,
        None => pve
            .cluster_resources(Some(ClusterResourceKind::Vm))
            .await?
            .into_iter()
            .find(|entry| entry.vmid == Some(vmid))
            .and_then(|entry| entry.node)
            .ok_or_else(|| http_err!(NOT_FOUND, "no such vmid"))?,
    };

    let upid = pve.stop_qemu_async(&node, vmid, Default::default()).await?;

    (remote, upid.to_string()).try_into()
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA },
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Perform a shutdown of a remote qemu vm.
pub async fn qemu_shutdown(
    remote: String,
    node: Option<String>,
    vmid: u32,
) -> Result<RemoteUpid, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

    // FIXME: The pve client should cache the resources and provide
    let node = match node {
        Some(node) => node,
        None => pve
            .cluster_resources(Some(ClusterResourceKind::Vm))
            .await?
            .into_iter()
            .find(|entry| entry.vmid == Some(vmid))
            .and_then(|entry| entry.node)
            .ok_or_else(|| http_err!(NOT_FOUND, "no such vmid"))?,
    };

    let upid = pve
        .shutdown_qemu_async(&node, vmid, Default::default())
        .await?;

    (remote, upid.to_string()).try_into()
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
            remote: { schema: REMOTE_ID_SCHEMA },
            target: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA },
            "target-vmid": {
                optional: true,
                schema: VMID_SCHEMA,
            },
            delete: {
                description: "Delete the original VM and related data after successful migration.",
                optional: true,
                default: false,
            },
            online: {
                type: bool,
                description: "Perform an online migration if the vm is running.",
                optional: true,
                default: false,
            },
            "target-storage": {
                description: "Mapping of source storages to target storages.",
            },
            "target-bridge": {
                description: "Mapping of source bridges to remote bridges.",
            },
            bwlimit: {
                description: "Override I/O bandwidth limit (in KiB/s).",
                optional: true,
            }
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::And(&[
            &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_MIGRATE, false),
            &Permission::Privilege(&["resource", "{target}", "guest", "{vmid}"], PRIV_RESOURCE_MIGRATE, false),
        ]),
    },
)]
/// Perform a remote migration of a VM.
#[allow(clippy::too_many_arguments)]
pub async fn qemu_remote_migrate(
    remote: String, // this is the source
    target: String, // this is the destination remote name
    node: Option<String>,
    vmid: u32,
    target_vmid: Option<u32>,
    delete: bool,
    online: bool,
    target_storage: String,
    target_bridge: String,
    bwlimit: Option<u64>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<RemoteUpid, Error> {
    if delete {
        check_guest_delete_perms(rpcenv, &remote, vmid)?;
    }

    let source = remote; // let's stick to "source" and "target" naming

    log::info!("remote migration requested");

    if source == target {
        bail!("source and destination clusters must be different");
    }

    let (remotes, _) = pdm_config::remotes::config()?;
    let target = get_remote(&remotes, &target)?;
    let source_conn = connect_to_remote(&remotes, &source)?;

    // FIXME: Cache resources call.
    let node = match node {
        Some(node) => node,
        None => source_conn
            .cluster_resources(Some(ClusterResourceKind::Vm))
            .await?
            .into_iter()
            .find(|entry| entry.vmid == Some(vmid))
            .and_then(|entry| entry.node)
            .ok_or_else(|| http_err!(NOT_FOUND, "no such vmid"))?,
    };

    // FIXME: For now we'll only try with the first node but we should probably try others, too, in
    // case some are offline?

    let target_node = target
        .nodes
        .first()
        .ok_or_else(|| format_err!("no nodes configured for target cluster"))?;
    let mut target_endpoint = format!(
        "host={host},apitoken=PVEAPIToken={userid}={secret}",
        host = target_node.hostname,
        userid = target.userid,
        secret = target.token,
    );
    if let Some(fp) = target_node.fingerprint.as_deref() {
        target_endpoint.reserve(fp.len() + ",fingerprint=".len());
        target_endpoint.push_str(",fingerprint=");
        target_endpoint.push_str(fp);
    }

    log::info!("forwarding remote migration requested");
    let params = pve_api_types::RemoteMigrateQemu {
        target_bridge,
        target_storage,
        delete: Some(delete),
        online: Some(online),
        target_vmid,
        target_endpoint,
        bwlimit,
    };
    log::info!("migrating vm {vmid} of node {node:?} with params {params:#?}");
    let upid = source_conn.remote_migrate_qemu(&node, vmid, params).await?;

    (source, upid.to_string()).try_into()
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA },
            state: { type: ConfigurationState },
            snapshot: {
                schema: SNAPSHOT_NAME_SCHEMA,
                optional: true,
            },
        },
    },
    returns: { type: pve_api_types::LxcConfig },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get the configuration of an lxc container from a remote. If a node is provided, the container
/// must be on that node, otherwise the node is determined automatically.
pub async fn lxc_get_config(
    remote: String,
    node: Option<String>,
    vmid: u32,
    state: ConfigurationState,
    snapshot: Option<String>,
) -> Result<pve_api_types::LxcConfig, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

    // FIXME: The pve client should cache the resources and provide
    let node = match node {
        Some(node) => node,
        None => pve
            .cluster_resources(Some(ClusterResourceKind::Vm))
            .await?
            .into_iter()
            .find(|entry| entry.vmid == Some(vmid))
            .and_then(|entry| entry.node)
            .ok_or_else(|| http_err!(NOT_FOUND, "no such vmid"))?,
    };

    Ok(pve
        .lxc_get_config(&node, vmid, state.current(), snapshot)
        .await?)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA },
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Start a remote lxc container.
pub async fn lxc_start(
    remote: String,
    node: Option<String>,
    vmid: u32,
) -> Result<RemoteUpid, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

    // FIXME: The pve client should cache the resources and provide
    let node = match node {
        Some(node) => node,
        None => pve
            .cluster_resources(Some(ClusterResourceKind::Vm))
            .await?
            .into_iter()
            .find(|entry| entry.vmid == Some(vmid))
            .and_then(|entry| entry.node)
            .ok_or_else(|| http_err!(NOT_FOUND, "no such vmid"))?,
    };

    let upid = pve.start_lxc_async(&node, vmid, Default::default()).await?;

    (remote, upid.to_string()).try_into()
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA },
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Stop a remote lxc container.
pub async fn lxc_stop(
    remote: String,
    node: Option<String>,
    vmid: u32,
) -> Result<RemoteUpid, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

    // FIXME: The pve client should cache the resources and provide
    let node = match node {
        Some(node) => node,
        None => pve
            .cluster_resources(Some(ClusterResourceKind::Vm))
            .await?
            .into_iter()
            .find(|entry| entry.vmid == Some(vmid))
            .and_then(|entry| entry.node)
            .ok_or_else(|| http_err!(NOT_FOUND, "no such vmid"))?,
    };

    let upid = pve.stop_lxc_async(&node, vmid, Default::default()).await?;

    (remote, upid.to_string()).try_into()
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA },
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Perform a shutdown of a remote lxc container.
pub async fn lxc_shutdown(
    remote: String,
    node: Option<String>,
    vmid: u32,
) -> Result<RemoteUpid, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

    // FIXME: The pve client should cache the resources and provide
    let node = match node {
        Some(node) => node,
        None => pve
            .cluster_resources(Some(ClusterResourceKind::Vm))
            .await?
            .into_iter()
            .find(|entry| entry.vmid == Some(vmid))
            .and_then(|entry| entry.node)
            .ok_or_else(|| http_err!(NOT_FOUND, "no such vmid"))?,
    };

    let upid = pve
        .shutdown_lxc_async(&node, vmid, Default::default())
        .await?;

    (remote, upid.to_string()).try_into()
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            target: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA },
            "target-vmid": {
                optional: true,
                schema: VMID_SCHEMA,
            },
            delete: {
                description: "Delete the original VM and related data after successful migration.",
                optional: true,
                default: false,
            },
            online: {
                type: bool,
                description: "Perform an online migration if the vm is running.",
                optional: true,
                default: false,
            },
            "target-storage": {
                description: "Mapping of source storages to target storages.",
            },
            "target-bridge": {
                description: "Mapping of source bridges to remote bridges.",
            },
            bwlimit: {
                description: "Override I/O bandwidth limit (in KiB/s).",
                optional: true,
            },
            restart: {
                description: "Perform a restart-migration.",
                optional: true,
            },
            timeout: {
                description: "Add a shutdown timeout for the restart-migration.",
                optional: true,
            },
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::And(&[
            &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_MIGRATE, false),
            &Permission::Privilege(&["resource", "{target}", "guest", "{vmid}"], PRIV_RESOURCE_MIGRATE, false),
        ]),
    },
)]
/// Perform a remote migration of an lxc container.
#[allow(clippy::too_many_arguments)]
pub async fn lxc_remote_migrate(
    remote: String, // this is the source
    target: String, // this is the destination remote name
    node: Option<String>,
    vmid: u32,
    target_vmid: Option<u32>,
    delete: bool,
    online: bool,
    target_storage: String,
    target_bridge: String,
    bwlimit: Option<u64>,
    restart: Option<bool>,
    timeout: Option<i64>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<RemoteUpid, Error> {
    if delete {
        check_guest_delete_perms(rpcenv, &remote, vmid)?;
    }

    let source = remote; // let's stick to "source" and "target" naming

    log::info!("remote migration requested");

    if source == target {
        bail!("source and destination clusters must be different");
    }

    let (remotes, _) = pdm_config::remotes::config()?;
    let target = get_remote(&remotes, &target)?;
    let source_conn = connect_to_remote(&remotes, &source)?;

    // FIXME: Cache resources call.
    let node = match node {
        Some(node) => node,
        None => source_conn
            .cluster_resources(Some(ClusterResourceKind::Vm))
            .await?
            .into_iter()
            .find(|entry| entry.vmid == Some(vmid))
            .and_then(|entry| entry.node)
            .ok_or_else(|| http_err!(NOT_FOUND, "no such vmid"))?,
    };

    // FIXME: For now we'll only try with the first node but we should probably try others, too, in
    // case some are offline?

    let target_node = target
        .nodes
        .first()
        .ok_or_else(|| format_err!("no nodes configured for target cluster"))?;
    let mut target_endpoint = format!(
        "host={host},apitoken=PVEAPIToken={userid}={secret}",
        host = target_node.hostname,
        userid = target.userid,
        secret = target.token,
    );
    if let Some(fp) = target_node.fingerprint.as_deref() {
        target_endpoint.reserve(fp.len() + ",fingerprint=".len());
        target_endpoint.push_str(",fingerprint=");
        target_endpoint.push_str(fp);
    }

    log::info!("forwarding remote migration requested");
    let params = pve_api_types::RemoteMigrateLxc {
        target_bridge,
        target_storage,
        delete: Some(delete),
        online: Some(online),
        target_vmid,
        target_endpoint,
        bwlimit: bwlimit.map(|limit| limit as f64),
        restart,
        timeout,
    };
    log::info!("migrating vm {vmid} of node {node:?} with params {params:#?}");
    let upid = source_conn.remote_migrate_lxc(&node, vmid, params).await?;

    (source, upid.to_string()).try_into()
}
