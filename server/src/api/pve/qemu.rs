use anyhow::{bail, format_err, Error};
use http::uri::Authority;

use proxmox_access_control::CachedUserInfo;
use proxmox_router::{
    http_bail, list_subdirs_api_method, Permission, Router, RpcEnvironment, SubdirMap,
};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use pdm_api_types::remotes::REMOTE_ID_SCHEMA;
use pdm_api_types::{
    Authid, ConfigurationState, RemoteUpid, CIDR_FORMAT, NODE_SCHEMA, PRIV_RESOURCE_AUDIT,
    PRIV_RESOURCE_MANAGE, PRIV_RESOURCE_MIGRATE, SNAPSHOT_NAME_SCHEMA, VMID_SCHEMA,
};

use pve_api_types::{QemuMigratePreconditions, StartQemuMigrationType};

use crate::api::pve::get_remote;

use super::{
    check_guest_delete_perms, check_guest_list_permissions, check_guest_permissions,
    connect_to_remote, find_node_for_vm, new_remote_upid,
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
    ("rrddata", &super::rrddata::QEMU_RRD_ROUTER),
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
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

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
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

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
    target_storage: String,
    target_bridge: String,
    bwlimit: Option<u64>,
    target_endpoint: Option<String>,
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

    // FIXME: For now we'll only try with the first node but we should probably try others, too, in
    // case some are offline?

    let target_node = target
        .nodes
        .iter()
        .find(|endpoint| match target_endpoint.as_deref() {
            Some(target) => target == endpoint.hostname,
            None => true,
        })
        .ok_or_else(|| match target_endpoint {
            Some(endpoint) => format_err!("{endpoint} not configured for target cluster"),
            None => format_err!("no nodes configured for target cluster"),
        })?;

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

    new_remote_upid(source, upid).await
}
