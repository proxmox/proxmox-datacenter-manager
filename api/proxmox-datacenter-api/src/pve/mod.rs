//! Manage PVE instances.

use anyhow::{bail, format_err, Error};

use proxmox_client::{Client, TlsOptions};
use proxmox_router::{http_err, list_subdirs_api_method, Router, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use pdm_api_types::remotes::{PveRemote, Remote, REMOTE_ID_SCHEMA};
use pdm_api_types::{
    ConfigurationState, RemoteUpid, NODE_SCHEMA, SNAPSHOT_NAME_SCHEMA, VMID_SCHEMA,
};
use pve_api_types::client::PveClient;
use pve_api_types::ClusterResourceKind;

use crate::remotes::get_remote;

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

pub fn connect(remote: &PveRemote) -> Result<PveClient<Client>, Error> {
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
)]
/// Query the remote's version.
///
/// FIXME: Should we add an option to explicitly query the entire cluster to get a full version
/// overview?
pub async fn list_nodes(
    remote: String,
) -> Result<Vec<pve_api_types::ClusterNodeIndexResponse>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    match get_remote(&remotes, &remote)? {
        Remote::Pve(pve) => Ok(connect(pve)?.list_nodes().await?),
    }
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
)]
/// Query the cluster's resources.
pub async fn cluster_resources(
    remote: String,
    kind: Option<ClusterResourceKind>,
) -> Result<Vec<pve_api_types::ClusterResource>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    match get_remote(&remotes, &remote)? {
        Remote::Pve(pve) => Ok(connect(pve)?.cluster_resources(kind).await?),
    }
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
)]
/// Query the remote's list of qemu VMs. If no node is provided, the all nodes are queried.
pub async fn list_qemu(
    remote: String,
    node: Option<String>,
) -> Result<Vec<pve_api_types::VmEntry>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = match get_remote(&remotes, &remote)? {
        Remote::Pve(pve) => connect(pve)?,
    };

    if let Some(node) = node {
        Ok(pve.list_qemu(&node, None).await?)
    } else {
        let mut entry = Vec::new();
        for node in pve.list_nodes().await? {
            entry.extend(pve.list_qemu(&node.node, None).await?);
        }
        Ok(entry)
    }
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
) -> Result<Vec<pve_api_types::LxcEntry>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = match get_remote(&remotes, &remote)? {
        Remote::Pve(pve) => connect(pve)?,
    };

    if let Some(node) = node {
        Ok(pve.list_lxc(&node).await?)
    } else {
        let mut entry = Vec::new();
        for node in pve.list_nodes().await? {
            entry.extend(pve.list_lxc(&node.node).await?);
        }
        Ok(entry)
    }
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

    let pve = match get_remote(&remotes, &remote)? {
        Remote::Pve(pve) => connect(pve)?,
    };

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
)]
/// Start a remote qemu vm.
pub async fn qemu_start(
    remote: String,
    node: Option<String>,
    vmid: u32,
) -> Result<RemoteUpid, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = match get_remote(&remotes, &remote)? {
        Remote::Pve(pve) => connect(pve)?,
    };

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
)]
/// Stop a remote qemu vm.
pub async fn qemu_stop(
    remote: String,
    node: Option<String>,
    vmid: u32,
) -> Result<RemoteUpid, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = match get_remote(&remotes, &remote)? {
        Remote::Pve(pve) => connect(pve)?,
    };

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
)]
/// Perform a shutdown of a remote qemu vm.
pub async fn qemu_shutdown(
    remote: String,
    node: Option<String>,
    vmid: u32,
) -> Result<RemoteUpid, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = match get_remote(&remotes, &remote)? {
        Remote::Pve(pve) => connect(pve)?,
    };

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
)]
/// Perform a shutdown of a remote qemu vm.
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
) -> Result<RemoteUpid, Error> {
    let source = remote; // let's stick to "source" and "target" naming

    log::info!("remote migration requested");

    if source == target {
        bail!("source and destination clusters must be different");
    }

    let (remotes, _) = pdm_config::remotes::config()?;

    let Remote::Pve(target) = get_remote(&remotes, &target)?;

    let source_conn = match get_remote(&remotes, &source)? {
        Remote::Pve(pve) => connect(pve)?,
    };

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

    let pve = match get_remote(&remotes, &remote)? {
        Remote::Pve(pve) => connect(pve)?,
    };

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
)]
/// Start a remote lxc container.
pub async fn lxc_start(
    remote: String,
    node: Option<String>,
    vmid: u32,
) -> Result<RemoteUpid, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = match get_remote(&remotes, &remote)? {
        Remote::Pve(pve) => connect(pve)?,
    };

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
)]
/// Stop a remote lxc container.
pub async fn lxc_stop(
    remote: String,
    node: Option<String>,
    vmid: u32,
) -> Result<RemoteUpid, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = match get_remote(&remotes, &remote)? {
        Remote::Pve(pve) => connect(pve)?,
    };

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
)]
/// Perform a shutdown of a remote lxc container.
pub async fn lxc_shutdown(
    remote: String,
    node: Option<String>,
    vmid: u32,
) -> Result<RemoteUpid, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = match get_remote(&remotes, &remote)? {
        Remote::Pve(pve) => connect(pve)?,
    };

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
