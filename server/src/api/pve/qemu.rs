use anyhow::{Context, Error, bail};

use proxmox_access_control::CachedUserInfo;
use proxmox_router::{
    ApiMethod, Permission, Router, RpcEnvironment, SubdirMap, http_bail, list_subdirs_api_method,
};
use proxmox_schema::{IntegerSchema, ObjectSchema, StringSchema, api};
use proxmox_sortable_macro::sortable;

use pdm_api_types::remotes::REMOTE_ID_SCHEMA;
use pdm_api_types::remotes::Remote;
use pdm_api_types::{
    Authid, CIDR_FORMAT, ConfigurationState, NODE_SCHEMA, PRIV_RESOURCE_AUDIT,
    PRIV_RESOURCE_MANAGE, PRIV_RESOURCE_MIGRATE, PRIV_SYS_CONSOLE, RemoteUpid,
    SNAPSHOT_NAME_SCHEMA, VMID_SCHEMA,
};

use pve_api_types::{PendingConfigValue, QemuMigratePreconditions, StartQemuMigrationType};
use serde_json::Value;

use crate::api::nodes::vncwebsocket::required_integer_param;
use crate::api::pve::get_remote;
use crate::api::remotes::shell::TermTicketType;

use super::{
    check_guest_delete_perms, check_guest_list_permissions, check_guest_permissions,
    connect_to_remote, connect_to_remote_by_id, find_node_for_vm, new_remote_upid,
};

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_QEMU)
    .match_all("vmid", &QEMU_VM_ROUTER);

const QEMU_VM_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(QEMU_VM_SUBDIRS))
    .subdirs(QEMU_VM_SUBDIRS);
#[sortable]
const QEMU_VM_SUBDIRS: SubdirMap = &sorted!([
    ("config", &Router::new().get(&API_METHOD_QEMU_GET_CONFIG)),
    ("pending", &Router::new().get(&API_METHOD_QEMU_GET_PENDING)),
    ("firewall", &super::firewall::QEMU_FW_ROUTER),
    ("rrddata", &super::rrddata::QEMU_RRD_ROUTER),
    ("start", &Router::new().post(&API_METHOD_QEMU_START)),
    ("status", &Router::new().get(&API_METHOD_QEMU_GET_STATUS)),
    ("stop", &Router::new().post(&API_METHOD_QEMU_STOP)),
    ("shutdown", &Router::new().post(&API_METHOD_QEMU_SHUTDOWN)),
    ("resume", &Router::new().post(&API_METHOD_QEMU_RESUME)),
    (
        "snapshot",
        &Router::new()
            .get(&API_METHOD_QEMU_LIST_SNAPSHOTS)
            .post(&API_METHOD_QEMU_CREATE_SNAPSHOT)
            .match_all("snapname", &QEMU_SNAPSHOT_ROUTER)
    ),
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
    (
        "termproxy",
        &Router::new().post(&API_METHOD_QEMU_SHELL_TICKET)
    ),
    (
        "vncwebsocket",
        &Router::new().upgrade(&API_METHOD_QEMU_WEBSOCKET)
    ),
    ("vncproxy", &Router::new().post(&API_METHOD_QEMU_VNC_TICKET)),
]);

const QEMU_SNAPSHOT_ROUTER: Router = Router::new()
    .delete(&API_METHOD_QEMU_DELETE_SNAPSHOT)
    .subdirs(QEMU_SNAPSHOT_SUBDIRS);
#[sortable]
const QEMU_SNAPSHOT_SUBDIRS: SubdirMap = &sorted!([
    (
        "config",
        &Router::new().put(&API_METHOD_QEMU_UPDATE_SNAPSHOT_CONFIG)
    ),
    (
        "rollback",
        &Router::new().post(&API_METHOD_QEMU_ROLLBACK_SNAPSHOT)
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

    let pve = connect_to_remote_by_id(&remote)?;

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
    let pve = connect_to_remote_by_id(&remote)?;

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
    // Note: the trait `ApiType` is not implemented for `PendingConfigValue` because it contains Value
    // returns: { description: "Configuration property with pending changes.", type: Array, items: { type: PendingConfigValue, }},
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get the pending configuration of a qemu VM from a remote. If a node is provided, the VM must be on that
/// node, otherwise the node is determined automatically.
pub async fn qemu_get_pending(
    remote: String,
    node: Option<String>,
    vmid: u32,
) -> Result<Vec<PendingConfigValue>, Error> {
    let pve = connect_to_remote_by_id(&remote)?;

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    Ok(pve.qemu_get_pending(&node, vmid).await?)
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
    let pve = connect_to_remote_by_id(&remote)?;

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
    let pve = connect_to_remote_by_id(&remote)?;

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    let upid = pve
        .start_qemu_async(&node, vmid, Default::default())
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
    let pve = connect_to_remote_by_id(&remote)?;

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

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
    let pve = connect_to_remote_by_id(&remote)?;

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    let upid = pve
        .shutdown_qemu_async(&node, vmid, Default::default())
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
/// Resume a paused or suspended remote qemu vm.
pub async fn qemu_resume(
    remote: String,
    node: Option<String>,
    vmid: u32,
) -> Result<RemoteUpid, Error> {
    let pve = connect_to_remote_by_id(&remote)?;

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    let upid = pve
        .resume_qemu_async(&node, vmid, Default::default())
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
        },
    },
    returns: {
        type: Array,
        description: "The list of snapshots, including the current state as 'current'.",
        items: { type: pve_api_types::QemuSnapshot },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// List the snapshots of a remote qemu vm.
pub async fn qemu_list_snapshots(
    remote: String,
    node: Option<String>,
    vmid: u32,
) -> Result<Vec<pve_api_types::QemuSnapshot>, Error> {
    let pve = connect_to_remote_by_id(&remote)?;
    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;
    Ok(pve.qemu_list_snapshots(&node, vmid).await?)
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
            vmstate: {
                type: bool,
                description: "Include the VM's RAM state, so the snapshot resumes exactly where it left off.",
                optional: true,
            },
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Create a snapshot of a remote qemu vm.
pub async fn qemu_create_snapshot(
    remote: String,
    node: Option<String>,
    vmid: u32,
    snapname: String,
    description: Option<String>,
    vmstate: Option<bool>,
) -> Result<RemoteUpid, Error> {
    let pve = connect_to_remote_by_id(&remote)?;
    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;
    let params = pve_api_types::CreateQemuSnapshot {
        snapname,
        description,
        vmstate,
    };
    let upid = pve.snapshot_qemu(&node, vmid, params).await?;
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
/// Delete a snapshot of a remote qemu vm.
pub async fn qemu_delete_snapshot(
    remote: String,
    node: Option<String>,
    vmid: u32,
    snapname: String,
) -> Result<RemoteUpid, Error> {
    let pve = connect_to_remote_by_id(&remote)?;
    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;
    let upid = pve
        .delete_qemu_snapshot(&node, vmid, &snapname, Default::default())
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
                description: "Start the guest after a successful rollback.",
            },
        },
    },
    returns: { type: RemoteUpid },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_MANAGE, false),
    },
)]
/// Roll back a remote qemu vm to a snapshot. This is destructive: it reverts the guest's disk and
/// configuration to that snapshot (and its RAM, if the snapshot includes it). Optionally starts the
/// guest afterwards.
pub async fn qemu_rollback_snapshot(
    remote: String,
    node: Option<String>,
    vmid: u32,
    snapname: String,
    start: Option<bool>,
) -> Result<RemoteUpid, Error> {
    let pve = connect_to_remote_by_id(&remote)?;
    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;
    let params = pve_api_types::RollbackQemuSnapshot { start };
    let upid = pve
        .rollback_qemu_snapshot(&node, vmid, &snapname, params)
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
/// Update a remote qemu vm snapshot's description. This is synchronous (no worker task).
pub async fn qemu_update_snapshot_config(
    remote: String,
    node: Option<String>,
    vmid: u32,
    snapname: String,
    description: Option<String>,
) -> Result<(), Error> {
    let pve = connect_to_remote_by_id(&remote)?;
    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;
    let params = pve_api_types::UpdateQemuSnapshotConfig { description };
    pve.update_qemu_snapshot_config(&node, vmid, &snapname, params)
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
            target: { schema: NODE_SCHEMA },
            vmid: { schema: VMID_SCHEMA },
            online: {
                type: bool,
                description: "Perform an online migration if the vm is running.",
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
    target_storage: Option<Vec<String>>,
    with_local_disks: Option<bool>,
) -> Result<RemoteUpid, Error> {
    log::info!("in-cluster migration requested for remote {remote:?} vm {vmid} to node {target:?}");

    let pve = connect_to_remote_by_id(&remote)?;

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
        with_conntrack_state: None,
    };
    let upid = pve.migrate_qemu(&node, vmid, params).await?;

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
    returns: { type: QemuMigratePreconditions }
)]
/// Qemu (local) migrate preconditions
async fn qemu_migrate_preconditions(
    remote: String,
    node: Option<String>,
    target: Option<String>,
    vmid: u32,
) -> Result<QemuMigratePreconditions, Error> {
    let pve = connect_to_remote_by_id(&remote)?;

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
    target_storage: Vec<String>,
    target_bridge: Vec<String>,
    bwlimit: Option<u64>,
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

    new_remote_upid(source, upid).await
}

fn encode_term_ticket_path(remote: &str, vmid: u32) -> String {
    format!("/qemu-shell/{remote}/{vmid}")
}

#[api(
    protected: true,
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            vmid: { schema: VMID_SCHEMA },
        },
    },
    returns: {
        type: Object,
        description: "Object with the user and ticket",
        properties: {
            user: {
                description: "User that obtained the VNC ticket.",
                type: String,
            },
            port: {
                description: "Always '0'.",
                type: Integer,
            }
        }
    },
    access: {
        description: "Restricted to users",
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_SYS_CONSOLE, false),
    }
)]
/// Call termproxy and return shell ticket
fn qemu_shell_ticket(
    remote: String,
    vmid: u32,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Value, Error> {
    crate::api::remotes::shell::create_term_ticket(
        rpcenv,
        move || encode_term_ticket_path(&remote, vmid),
        move || TermTicketType::QemuTerm,
    )
}

#[sortable]
pub const API_METHOD_QEMU_WEBSOCKET: ApiMethod = ApiMethod::new(
    &proxmox_router::ApiHandler::AsyncHttp(&upgrade_to_websocket),
    &ObjectSchema::new(
        "Upgraded to websocket",
        &sorted!([
            ("remote", false, &REMOTE_ID_SCHEMA),
            ("vmid", false, &VMID_SCHEMA),
            (
                "vncticket",
                false,
                &StringSchema::new("Terminal ticket").schema()
            ),
            ("port", false, &IntegerSchema::new("Terminal port").schema()),
        ]),
    ),
)
.access(
    Some("The user needs Sys.Console on /resource/{remote}/node/{node}."),
    &Permission::Privilege(
        &["resource", "{remote}", "node", "{node}"],
        PRIV_SYS_CONSOLE,
        false,
    ),
);

fn upgrade_to_websocket(
    parts: http::request::Parts,
    req_body: hyper::body::Incoming,
    param: Value,
    _info: &ApiMethod,
    rpcenv: Box<dyn RpcEnvironment>,
) -> proxmox_router::ApiResponseFuture {
    Box::pin(upgrade_to_websocket_do(parts, req_body, param, rpcenv))
}

crate::api::remotes::shell::upgrade_to_websocket_impl! {
    upgrade_to_websocket_do,
    |param: &Value| -> Result<u32, Error> {
        Ok(u32::try_from(required_integer_param(param, "vmid")?)?)
    },
    |remote, &vmid| encode_term_ticket_path(remote, vmid),
    TermTicketType,
    async |
        remote: &Remote,
        vmid: &u32,
        kind: TermTicketType,
    | -> Result<(String, i64, String, bool), Error> {
        if remote.ty != pdm_api_types::remotes::RemoteType::Pve {
            bail!("expected a PVE remote type for console ticket");
        }
        let vmid = *vmid;
        let pve = crate::connection::make_pve_client(remote)?;
        let node = find_node_for_vm(None, vmid, pve.as_ref()).await?;
        match kind {
            TermTicketType::QemuTerm => {
                let param = pve_api_types::QemuTermProxy { serial: None };
                let ticket = pve.qemu_termproxy(&node, vmid, param).await?;
                Ok((ticket.ticket, ticket.port, node, true))
            }
            TermTicketType::QemuVnc { ticket, port } => {
                log::error!("Here on {node} with {port} and {ticket:?}");
                Ok((ticket, port, node, false))
            }
            _ => bail!("expected qemu term/vnc ticket, got '{kind}'"),
        }
    },
    |vmid: u32, node: String, ticket: &str, port: i64| {
         proxmox_client::ApiPathBuilder::new(format!(
            "/api2/json/nodes/{node}/qemu/{vmid}/vncwebsocket"
        ))
        .arg("vncticket", ticket)
        .arg("port", port)
        .build()
    },
}

#[api(
    protected: true,
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            vmid: { schema: VMID_SCHEMA },
            websocket: {
                description: "Prepare for websocket upgrade",
                type: Boolean,
                optional: true,
                default: false,
            },
        },
    },
    returns: {
        type: Object,
        description: "Object with the user and ticket",
        properties: {
            user: {
                description: "User that obtained the VNC ticket.",
                type: String,
            },
            port: {
                description: "Always '0'.",
                type: Integer,
            },
            password: {
                description: "VNC protocol password for this session.",
                type: String,
                optional: true,
            },
        }
    },
    access: {
        description: "Restricted to users",
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_SYS_CONSOLE, false),
    }
)]
/// Call vncproxy and return shell ticket.
async fn qemu_vnc_ticket(
    remote: String,
    vmid: u32,
    websocket: bool,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Value, Error> {
    let (remotes, _digest) = pdm_config::remotes::config()?;
    let remote = get_remote(&remotes, &remote)?;

    let pve = crate::connection::make_pve_client(remote)?;

    let node = find_node_for_vm(None, vmid, pve.as_ref()).await?;

    let param = pve_api_types::QemuVncProxy {
        generate_password: None,
        websocket: websocket.then_some(true),
    };
    let ticket = pve.qemu_vncproxy(&node, vmid, param).await?;

    let mut output = crate::api::remotes::shell::create_term_ticket(
        rpcenv,
        move || encode_term_ticket_path(&remote.id, vmid),
        move || TermTicketType::QemuVnc {
            ticket: ticket.ticket,
            port: ticket.port,
        },
    )?;

    if let Some(password) = ticket.password {
        output["password"] = password.into();
    }

    Ok(output)
}
