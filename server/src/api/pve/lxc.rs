use anyhow::{bail, Context, Error};

use proxmox_access_control::CachedUserInfo;
use proxmox_router::{
    http_bail, list_subdirs_api_method, Permission, Router, RpcEnvironment, SubdirMap,
};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;
use pve_api_types::PendingConfigValue;

use pdm_api_types::remotes::REMOTE_ID_SCHEMA;
use pdm_api_types::{
    Authid, ConfigurationState, RemoteUpid, NODE_SCHEMA, PRIV_RESOURCE_AUDIT, PRIV_RESOURCE_MANAGE,
    PRIV_RESOURCE_MIGRATE, SNAPSHOT_NAME_SCHEMA, VMID_SCHEMA,
};

use crate::api::pve::get_remote;

use super::{
    check_guest_delete_perms, check_guest_list_permissions, check_guest_permissions,
    connect_to_remote, connect_to_remote_by_id, new_remote_upid,
};

use super::find_node_for_vm;

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_LXC)
    .match_all("vmid", &LXC_VM_ROUTER);

const LXC_VM_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(LXC_VM_SUBDIRS))
    .subdirs(LXC_VM_SUBDIRS);
#[sortable]
const LXC_VM_SUBDIRS: SubdirMap = &sorted!([
    ("config", &Router::new().get(&API_METHOD_LXC_GET_CONFIG)),
    ("pending", &Router::new().get(&API_METHOD_LXC_GET_PENDING)),
    ("firewall", &super::firewall::LXC_FW_ROUTER),
    ("rrddata", &super::rrddata::LXC_RRD_ROUTER),
    ("start", &Router::new().post(&API_METHOD_LXC_START)),
    ("status", &Router::new().get(&API_METHOD_LXC_GET_STATUS)),
    ("stop", &Router::new().post(&API_METHOD_LXC_STOP)),
    ("shutdown", &Router::new().post(&API_METHOD_LXC_SHUTDOWN)),
    (
        "snapshot",
        &Router::new()
            .get(&API_METHOD_LXC_LIST_SNAPSHOTS)
            .post(&API_METHOD_LXC_CREATE_SNAPSHOT)
            .match_all("snapname", &LXC_SNAPSHOT_ROUTER)
    ),
    ("migrate", &Router::new().post(&API_METHOD_LXC_MIGRATE)),
    (
        "remote-migrate",
        &Router::new().post(&API_METHOD_LXC_REMOTE_MIGRATE)
    ),
]);

const LXC_SNAPSHOT_ROUTER: Router = Router::new()
    .delete(&API_METHOD_LXC_DELETE_SNAPSHOT)
    .subdirs(LXC_SNAPSHOT_SUBDIRS);
#[sortable]
const LXC_SNAPSHOT_SUBDIRS: SubdirMap = &sorted!([
    (
        "config",
        &Router::new().put(&API_METHOD_LXC_UPDATE_SNAPSHOT_CONFIG)
    ),
    (
        "rollback",
        &Router::new().post(&API_METHOD_LXC_ROLLBACK_SNAPSHOT)
    )
]);

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
        items: { type: pve_api_types::LxcEntry },
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

    let pve = connect_to_remote_by_id(&remote)?;

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
    let pve = connect_to_remote_by_id(&remote)?;

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
    // Note: the trait `ApiType` is not implemented for `PendingConfigValue` because it contains Value
    // returns: { description: "Configuration property with pending changes.", type: Array, items: { type: PendingConfigValue, }},
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get the pending configuration of a lxc container from a remote. If a node is provided, the container must be on that
/// node, otherwise the node is determined automatically.
pub async fn lxc_get_pending(
    remote: String,
    node: Option<String>,
    vmid: u32,
) -> Result<Vec<PendingConfigValue>, Error> {
    let pve = connect_to_remote_by_id(&remote)?;

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    Ok(pve.lxc_get_pending(&node, vmid).await?)
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
    returns: { type: pve_api_types::LxcStatus },
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
    let pve = connect_to_remote_by_id(&remote)?;

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
    let pve = connect_to_remote_by_id(&remote)?;

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    let upid = pve.start_lxc_async(&node, vmid, Default::default()).await?;

    new_remote_upid(remote, upid).await
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
    returns: {
        type: Array,
        description: "The list of snapshots, including the current state as 'current'.",
        items: { type: pve_api_types::LxcSnapshot },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// List the snapshots of a remote lxc container.
pub async fn lxc_list_snapshots(
    remote: String,
    node: Option<String>,
    vmid: u32,
) -> Result<Vec<pve_api_types::LxcSnapshot>, Error> {
    let pve = connect_to_remote_by_id(&remote)?;
    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;
    Ok(pve.lxc_list_snapshots(&node, vmid).await?)
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
            snapname: { schema: SNAPSHOT_NAME_SCHEMA },
            description: {
                type: String,
                description: "A textual description or comment.",
                optional: true,
            },
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Create a snapshot of a remote lxc container.
pub async fn lxc_create_snapshot(
    remote: String,
    node: Option<String>,
    vmid: u32,
    snapname: String,
    description: Option<String>,
) -> Result<RemoteUpid, Error> {
    let pve = connect_to_remote_by_id(&remote)?;
    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;
    let params = pve_api_types::CreateLxcSnapshot {
        snapname,
        description,
    };
    let upid = pve.snapshot_lxc(&node, vmid, params).await?;
    new_remote_upid(remote, upid).await
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
            snapname: { schema: SNAPSHOT_NAME_SCHEMA },
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Delete a snapshot of a remote lxc container.
pub async fn lxc_delete_snapshot(
    remote: String,
    node: Option<String>,
    vmid: u32,
    snapname: String,
) -> Result<RemoteUpid, Error> {
    let pve = connect_to_remote_by_id(&remote)?;
    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;
    let upid = pve
        .delete_lxc_snapshot(&node, vmid, &snapname, Default::default())
        .await?;
    new_remote_upid(remote, upid).await
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
            snapname: { schema: SNAPSHOT_NAME_SCHEMA },
            start: {
                type: bool,
                optional: true,
                description: "Start the container after a successful rollback.",
            },
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Roll back a remote lxc container to a snapshot. This is destructive: it reverts the container's
/// disk and configuration to that snapshot. Optionally starts the container afterwards.
pub async fn lxc_rollback_snapshot(
    remote: String,
    node: Option<String>,
    vmid: u32,
    snapname: String,
    start: Option<bool>,
) -> Result<RemoteUpid, Error> {
    let pve = connect_to_remote_by_id(&remote)?;
    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;
    let params = pve_api_types::RollbackLxcSnapshot { start };
    let upid = pve
        .rollback_lxc_snapshot(&node, vmid, &snapname, params)
        .await?;
    new_remote_upid(remote, upid).await
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
            snapname: { schema: SNAPSHOT_NAME_SCHEMA },
            description: {
                type: String,
                description: "A textual description or comment.",
                optional: true,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Update a remote lxc container snapshot's description. This is synchronous (no worker task).
pub async fn lxc_update_snapshot_config(
    remote: String,
    node: Option<String>,
    vmid: u32,
    snapname: String,
    description: Option<String>,
) -> Result<(), Error> {
    let pve = connect_to_remote_by_id(&remote)?;
    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;
    let params = pve_api_types::UpdateLxcSnapshotConfig { description };
    pve.update_lxc_snapshot_config(&node, vmid, &snapname, params)
        .await?;
    Ok(())
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
    let pve = connect_to_remote_by_id(&remote)?;

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    let upid = pve.stop_lxc_async(&node, vmid, Default::default()).await?;

    new_remote_upid(remote, upid).await
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
    let pve = connect_to_remote_by_id(&remote)?;

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    let upid = pve
        .shutdown_lxc_async(&node, vmid, Default::default())
        .await?;

    new_remote_upid(remote, upid).await
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
                description: "List of storage mappings",
                optional: true,
                items: {
                    description: "Mappings of source storages to target storages.",
                    type: String,
                },
                type: Array,
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
    target_storage: Option<Vec<String>>,
    timeout: Option<i64>,
) -> Result<RemoteUpid, Error> {
    let bwlimit = bwlimit.map(|n| n as f64);

    log::info!("in-cluster migration requested for remote {remote:?} ct {vmid} to node {target:?}");

    let pve = connect_to_remote_by_id(&remote)?;

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

    new_remote_upid(remote, upid).await
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
                description: "List of storage mappings",
                items: {
                    description: "Mappings of source storages to target storages.",
                    type: String,
                },
                type: Array,
            },
            "target-bridge": {
                description: "List of bridge mappings",
                items: {
                    description: "Mappings of source bridges to remote bridges.",
                    type: String,
                },
                type: Array,
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
            // TODO better to change remote migration to proxy to node?
            "target-endpoint": {
                type: String,
                optional: true,
                description: "The target endpoint to use for the connection.",
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
    target_storage: Vec<String>,
    target_bridge: Vec<String>,
    bwlimit: Option<u64>,
    restart: Option<bool>,
    timeout: Option<i64>,
    target_endpoint: Option<String>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<RemoteUpid, Error> {
    let user_info = CachedUserInfo::new()?;
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .context("no authid available")?
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
            FORBIDDEN,
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

    let target_node = super::select_migration_target_node(target, target_endpoint.as_deref())?;
    let target_endpoint = super::build_migration_endpoint(target, target_node)?;

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

    new_remote_upid(source, upid).await
}
