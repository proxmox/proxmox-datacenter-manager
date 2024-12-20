//! Manage PVE instances.

use std::sync::Arc;

use anyhow::{bail, format_err, Error};
use http::uri::Authority;

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
    Authid, ConfigurationState, RemoteUpid, CIDR_FORMAT, HOST_OPTIONAL_PORT_FORMAT, NODE_SCHEMA,
    PRIV_RESOURCE_AUDIT, PRIV_RESOURCE_DELETE, PRIV_RESOURCE_MANAGE, PRIV_RESOURCE_MIGRATE,
    PRIV_SYS_MODIFY, SNAPSHOT_NAME_SCHEMA, VMID_SCHEMA,
};

use pve_api_types::client::PveClient;
use pve_api_types::{
    ClusterResourceKind, ClusterResourceType, ListRealm, PveUpid, QemuMigratePreconditions,
    StartQemuMigrationType,
};

use super::resources::{map_pve_lxc, map_pve_node, map_pve_qemu, map_pve_storage};

use crate::{connection, task_cache};

mod node;
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
    ("rrddata", &rrddata::LXC_RRD_ROUTER),
    ("start", &Router::new().post(&API_METHOD_LXC_START)),
    ("status", &Router::new().get(&API_METHOD_LXC_GET_STATUS)),
    ("stop", &Router::new().post(&API_METHOD_LXC_STOP)),
    ("shutdown", &Router::new().post(&API_METHOD_LXC_SHUTDOWN)),
    ("migrate", &Router::new().post(&API_METHOD_LXC_MIGRATE)),
    (
        "remote-migrate",
        &Router::new().post(&API_METHOD_LXC_REMOTE_MIGRATE)
    ),
]);

const NODES_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_NODES)
    .match_all("node", &node::ROUTER);

const QEMU_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_QEMU)
    .match_all("vmid", &QEMU_VM_ROUTER);

const QEMU_VM_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(QEMU_VM_SUBDIRS))
    .subdirs(QEMU_VM_SUBDIRS);
#[sortable]
const QEMU_VM_SUBDIRS: SubdirMap = &sorted!([
    ("config", &Router::new().get(&API_METHOD_QEMU_GET_CONFIG)),
    ("rrddata", &rrddata::QEMU_RRD_ROUTER),
    ("start", &Router::new().post(&API_METHOD_QEMU_START)),
    ("status", &Router::new().get(&API_METHOD_QEMU_GET_STATUS)),
    ("stop", &Router::new().post(&API_METHOD_QEMU_STOP)),
    ("shutdown", &Router::new().post(&API_METHOD_QEMU_SHUTDOWN)),
    (
        "migrate",
        &Router::new()
            .get(&API_METHOD_QEMU_MIGRATE_PRECONDITIONS)
            .post(&API_METHOD_QEMU_MIGRATE)
    ),
    (
        "remote-migrate",
        &Router::new().post(&API_METHOD_QEMU_REMOTE_MIGRATE)
    ),
]);

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
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Query the remote's list of qemu VMs. If no node is provided, the all nodes are queried.
pub async fn list_qemu(
    remote: String,
    node: Option<String>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<pve_api_types::VmEntry>, Error> {
    // FIXME: top_level_allowed is always true because of schema check above, replace with Anybody
    // and fine-grained checks once those are implemented for all API calls..
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
        .filter(|entry| {
            check_guest_permissions(
                &auth_id,
                &user_info,
                &remote,
                PRIV_RESOURCE_AUDIT,
                entry.vmid,
            )
        })
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
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Query the remote's list of lxc containers. If no node is provided, the all nodes are queried.
pub async fn list_lxc(
    remote: String,
    node: Option<String>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<pve_api_types::LxcEntry>, Error> {
    // FIXME: top_level_allowed is always true because of schema check above, replace with Anybody
    // and fine-grained checks once those are implemented for all API calls..
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
        .filter(|entry| {
            check_guest_permissions(
                &auth_id,
                &user_info,
                &remote,
                PRIV_RESOURCE_AUDIT,
                entry.vmid,
            )
        })
        .collect())
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

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

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
    returns: { type: pve_api_types::QemuStatus },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get the status of a qemu VM from a remote. If a node is provided, the VM must be on that
/// node, otherwise the node is determined automatically.
pub async fn qemu_get_status(
    remote: String,
    node: Option<String>,
    vmid: u32,
) -> Result<pve_api_types::QemuStatus, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    Ok(pve.qemu_get_status(&node, vmid).await?)
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

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    let upid = pve
        .start_qemu_async(&node, vmid, Default::default())
        .await?;

    new_remote_upid(remote, upid)
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

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    let upid = pve.stop_qemu_async(&node, vmid, Default::default()).await?;

    new_remote_upid(remote, upid)
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

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    let upid = pve
        .shutdown_qemu_async(&node, vmid, Default::default())
        .await?;

    //(remote, upid.to_string()).try_into()
    new_remote_upid(remote, upid)
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
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            target: { schema: NODE_SCHEMA },
            vmid: { schema: VMID_SCHEMA },
            online: {
                type: bool,
                description: "Perform an online migration if the vm is running.",
                optional: true,
            },
            "target-storage": {
                description: "Mapping of source storages to target storages.",
                optional: true,
            },
            bwlimit: {
                description: "Override I/O bandwidth limit (in KiB/s).",
                optional: true,
            },
            "migration-network": {
                description: "CIDR of the (sub) network that is used for migration.",
                type: String,
                format: &CIDR_FORMAT,
                optional: true,
            },
            "migration-type": {
                type: StartQemuMigrationType,
                optional: true,
            },
            force: {
                description: "Allow to migrate VMs with local devices.",
                optional: true,
                default: false,
            },
            "with-local-disks": {
                description: "Enable live storage migration for local disks.",
                optional: true,
            },
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::And(&[
            &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_MIGRATE, false),
        ]),
    },
)]
/// Perform an in-cluster migration of a VM.
#[allow(clippy::too_many_arguments)]
pub async fn qemu_migrate(
    remote: String,
    node: Option<String>,
    vmid: u32,
    bwlimit: Option<u64>,
    force: Option<bool>,
    migration_network: Option<String>,
    migration_type: Option<StartQemuMigrationType>,
    online: Option<bool>,
    target: String,
    target_storage: Option<String>,
    with_local_disks: Option<bool>,
) -> Result<RemoteUpid, Error> {
    log::info!("in-cluster migration requested for remote {remote:?} vm {vmid} to node {target:?}");

    let (remotes, _) = pdm_config::remotes::config()?;
    let pve = connect_to_remote(&remotes, &remote)?;

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    if node == target {
        bail!("refusing migration to the same node");
    }

    let params = pve_api_types::MigrateQemu {
        bwlimit,
        force,
        migration_network,
        migration_type,
        online,
        target,
        targetstorage: target_storage,
        with_local_disks,
    };
    let upid = pve.migrate_qemu(&node, vmid, params).await?;
    //(remote, upid.to_string()).try_into()
    new_remote_upid(remote, upid)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            target: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA },
        }
    },
    access: {
        permission: &Permission::And(&[
            &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_MIGRATE, false),
        ]),
    },
)]
/// Qemu (local) migrate preconditions
async fn qemu_migrate_preconditions(
    remote: String,
    node: Option<String>,
    target: Option<String>,
    vmid: u32,
) -> Result<QemuMigratePreconditions, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let pve = connect_to_remote(&remotes, &remote)?;

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    let res = pve.qemu_migrate_preconditions(&node, vmid, target).await?;
    Ok(res)
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
        permission:
            &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_MIGRATE, false),
        description: "requires PRIV_RESOURCE_MIGRATE on /resource/{remote}/guest/{vmid} for source and target remove and vmid",
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
    let user_info = CachedUserInfo::new()?;
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .ok_or_else(|| format_err!("no authid available"))?
        .parse()?;
    let target_privs = user_info.lookup_privs(
        &auth_id,
        &[
            "resource",
            &target,
            "guest",
            &target_vmid.unwrap_or(vmid).to_string(),
        ],
    );
    if target_privs & PRIV_RESOURCE_MIGRATE == 0 {
        http_bail!(
            UNAUTHORIZED,
            "missing PRIV_RESOURCE_MIGRATE on target remote+vmid"
        );
    }

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

    let node = find_node_for_vm(node, vmid, source_conn.as_ref()).await?;

    // FIXME: For now we'll only try with the first node but we should probably try others, too, in
    // case some are offline?

    let target_node = target
        .nodes
        .first()
        .ok_or_else(|| format_err!("no nodes configured for target cluster"))?;
    let target_host_port: Authority = target_node.hostname.parse()?;
    let mut target_endpoint = format!(
        "host={host},port={port},apitoken=PVEAPIToken={authid}={secret}",
        host = target_host_port.host(),
        authid = target.authid,
        secret = target.token,
        port = target_host_port.port_u16().unwrap_or(8006),
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
    log::info!("migrating vm {vmid} of node {node:?}");
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

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

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
    returns: { type: pve_api_types::QemuStatus },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get the status of an LXC guest from a remote. If a node is provided, the guest must be on that
/// node, otherwise the node is determined automatically.
pub async fn lxc_get_status(
    remote: String,
    node: Option<String>,
    vmid: u32,
) -> Result<pve_api_types::LxcStatus, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    Ok(pve.lxc_get_status(&node, vmid).await?)
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

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    let upid = pve.start_lxc_async(&node, vmid, Default::default()).await?;

    new_remote_upid(remote, upid)
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

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    let upid = pve.stop_lxc_async(&node, vmid, Default::default()).await?;

    new_remote_upid(remote, upid)
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

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    let upid = pve
        .shutdown_lxc_async(&node, vmid, Default::default())
        .await?;

    new_remote_upid(remote, upid)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            target: { schema: NODE_SCHEMA },
            vmid: { schema: VMID_SCHEMA },
            online: {
                type: bool,
                description: "Attempt an online migration if the container is running.",
                optional: true,
            },
            restart: {
                type: bool,
                description: "Perform a restart-migration if the container is running.",
                optional: true,
            },
            "target-storage": {
                description: "Mapping of source storages to target storages.",
                optional: true,
            },
            bwlimit: {
                description: "Override I/O bandwidth limit (in KiB/s).",
                optional: true,
            },
            timeout: {
                description: "Shutdown timeout for restart-migrations.",
                optional: true,
            },
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::And(&[
            &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_MIGRATE, false),
        ]),
    },
)]
/// Perform an in-cluster migration of a VM.
#[allow(clippy::too_many_arguments)]
pub async fn lxc_migrate(
    remote: String,
    node: Option<String>,
    vmid: u32,
    bwlimit: Option<u64>,
    restart: Option<bool>,
    online: Option<bool>,
    target: String,
    target_storage: Option<String>,
    timeout: Option<i64>,
) -> Result<RemoteUpid, Error> {
    let bwlimit = bwlimit.map(|n| n as f64);

    log::info!("in-cluster migration requested for remote {remote:?} ct {vmid} to node {target:?}");

    let (remotes, _) = pdm_config::remotes::config()?;
    let pve = connect_to_remote(&remotes, &remote)?;

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    if node == target {
        bail!("refusing migration to the same node");
    }

    let params = pve_api_types::MigrateLxc {
        bwlimit,
        online,
        restart,
        target,
        target_storage,
        timeout,
    };
    let upid = pve.migrate_lxc(&node, vmid, params).await?;

    new_remote_upid(remote, upid)
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
        permission:
            &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_MIGRATE, false),
        description: "requires PRIV_RESOURCE_MIGRATE on /resource/{remote}/guest/{vmid} for source and target remove and vmid",
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
    let user_info = CachedUserInfo::new()?;
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .ok_or_else(|| format_err!("no authid available"))?
        .parse()?;
    let target_privs = user_info.lookup_privs(
        &auth_id,
        &[
            "resource",
            &target,
            "guest",
            &target_vmid.unwrap_or(vmid).to_string(),
        ],
    );
    if target_privs & PRIV_RESOURCE_MIGRATE == 0 {
        http_bail!(
            UNAUTHORIZED,
            "missing PRIV_RESOURCE_MIGRATE on target remote+vmid"
        );
    }
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

    let node = find_node_for_vm(node, vmid, source_conn.as_ref()).await?;

    // FIXME: For now we'll only try with the first node but we should probably try others, too, in
    // case some are offline?

    let target_node = target
        .nodes
        .first()
        .ok_or_else(|| format_err!("no nodes configured for target cluster"))?;
    let target_host_port: Authority = target_node.hostname.parse()?;
    let mut target_endpoint = format!(
        "host={host},port={port},apitoken=PVEAPIToken={authid}={secret}",
        host = target_host_port.host(),
        authid = target.authid,
        secret = target.token,
        port = target_host_port.port_u16().unwrap_or(8006),
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
    log::info!("migrating vm {vmid} of node {node:?}");
    let upid = source_conn.remote_migrate_lxc(&node, vmid, params).await?;

    new_remote_upid(source, upid)
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
