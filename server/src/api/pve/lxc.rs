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
    Authid, ConfigurationState, RemoteUpid, NODE_SCHEMA, PRIV_RESOURCE_AUDIT, PRIV_RESOURCE_MANAGE,
    PRIV_RESOURCE_MIGRATE, SNAPSHOT_NAME_SCHEMA, VMID_SCHEMA,
};

use crate::api::pve::get_remote;

use super::{
    check_guest_delete_perms, check_guest_list_permissions, check_guest_permissions,
    connect_to_remote, new_remote_upid,
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
    ("rrddata", &super::rrddata::LXC_RRD_ROUTER),
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
    target_storage: String,
    target_bridge: String,
    bwlimit: Option<u64>,
    restart: Option<bool>,
    timeout: Option<i64>,
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
