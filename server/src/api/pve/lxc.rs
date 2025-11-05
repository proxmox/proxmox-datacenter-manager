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
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

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
            vmid: { schema: VMID_SCHEMA },
            migrate: {
                type: pve_api_types::MigrateLxc,
                flatten: true,
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
    migrate: pve_api_types::MigrateLxc,
) -> Result<RemoteUpid, Error> {
    log::info!(
        "in-cluster migration requested for remote {remote:?} ct {vmid} to node {:?}",
        migrate.target
    );

    let (remotes, _) = pdm_config::remotes::config()?;
    let pve = connect_to_remote(&remotes, &remote)?;

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    if node == migrate.target {
        bail!("refusing migration to the same node");
    }

    let upid = pve.migrate_lxc(&node, vmid, migrate).await?;

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
            // TODO better to change remote migration to proxy to node?
            remote_migrate: {
                type: pve_api_types::RemoteMigrateLxc,
                flatten: true,
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
    remote_migrate: pve_api_types::RemoteMigrateLxc,
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
            &remote_migrate.target_vmid.unwrap_or(vmid).to_string(),
        ],
    );
    if target_privs & PRIV_RESOURCE_MIGRATE == 0 {
        http_bail!(
            FORBIDDEN,
            "missing PRIV_RESOURCE_MIGRATE on target remote+vmid"
        );
    }
    if remote_migrate.delete.unwrap_or_default() {
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

    // TODO: target_endpoint optional? if single node i guess
    let target_node = target
        .nodes
        .iter()
        .find(
            |endpoint| match Some(remote_migrate.target_endpoint.clone()).as_deref() {
                Some(target) => target == endpoint.hostname,
                None => true,
            },
        )
        .ok_or_else(|| match Some(remote_migrate.target_endpoint.clone()) {
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
    log::info!("migrating vm {vmid} of node {node:?}");
    let upid = source_conn
        .remote_migrate_lxc(&node, vmid, remote_migrate)
        .await?;

    new_remote_upid(source, upid).await
}
