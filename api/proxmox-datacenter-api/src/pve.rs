//! Manage PVE instances.

use anyhow::{format_err, Error};

use proxmox_client::Environment;
use proxmox_router::{http_err, list_subdirs_api_method, Router, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use pdm_api_types::{
    ConfigurationState, PveRemote, Remote, NODE_SCHEMA, REMOTE_ID_SCHEMA, SNAPSHOT_NAME_SCHEMA,
    VMID_SCHEMA,
};
use pve_client::types::ClusterResourceKind;

use super::remotes::get_remote;

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
]);

const NODES_ROUTER: Router = Router::new().get(&API_METHOD_LIST_NODES);
const RESOURCES_ROUTER: Router = Router::new().get(&API_METHOD_CLUSTER_RESOURCES);

const QEMU_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_QEMU)
    .match_all("vmid", &QEMU_VM_ROUTER);

const QEMU_VM_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(QEMU_VM_SUBDIRS))
    .subdirs(QEMU_VM_SUBDIRS);
#[sortable]
const QEMU_VM_SUBDIRS: SubdirMap =
    &sorted!([("config", &Router::new().get(&API_METHOD_QEMU_GET_CONFIG)),]);

const LXC_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_LXC)
    .match_all("vmid", &LXC_VM_ROUTER);

const LXC_VM_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(LXC_VM_SUBDIRS))
    .subdirs(LXC_VM_SUBDIRS);
#[sortable]
const LXC_VM_SUBDIRS: SubdirMap =
    &sorted!([("config", &Router::new().get(&API_METHOD_LXC_GET_CONFIG)),]);

pub type PveClient = pve_client::Client<PveEnv>;

pub struct PveEnv {
    remote: PveRemote,
}

impl PveEnv {
    pub fn new(remote: PveRemote) -> Self {
        Self { remote }
    }
}

impl Environment for PveEnv {
    type Error = Error;

    fn query_userid(&self, _: &http::Uri) -> Result<String, Error> {
        Ok(self.remote.userid.clone())
    }

    fn load_ticket(&self, _: &http::Uri, _userid: &str) -> Result<Option<Vec<u8>>, Error> {
        Ok(Some(self.remote.token.as_bytes().to_vec()))
    }
}

pub fn connect(remote: &PveRemote) -> Result<PveClient, Error> {
    let node = remote
        .nodes
        .first()
        .ok_or_else(|| format_err!("no nodes configured for remote"))?;

    let mut options = pve_client::Options::new();
    if let Some(fp) = &node.fingerprint {
        options = options.tls_fingerprint_str(fp)?;
    }

    let client = PveClient::new(PveEnv::new(remote.clone()), &node.hostname, options)?;

    client.client.use_api_token(proxmox_client::Token {
        userid: remote.userid.clone(),
        prefix: "PVEAPIToken".to_string(),
        value: remote.token.to_string(),
    });

    Ok(client)
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
        items: { type: pve_client::types::ClusterNodeIndexResponse },
    },
)]
/// Query the remote's version.
///
/// FIXME: Should we add an option to explicitly query the entire cluster to get a full version
/// overview?
pub async fn list_nodes(
    remote: String,
) -> Result<Vec<pve_client::types::ClusterNodeIndexResponse>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    match get_remote(&remotes, &remote)? {
        Remote::Pve(pve) => connect(pve)?.list_nodes().await,
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
        items: { type: pve_client::types::ClusterResource },
    },
)]
/// Query the cluster's resources.
pub async fn cluster_resources(
    remote: String,
    kind: Option<ClusterResourceKind>,
) -> Result<Vec<pve_client::types::ClusterResource>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    match get_remote(&remotes, &remote)? {
        Remote::Pve(pve) => connect(pve)?.cluster_resources(kind).await,
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
        items: { type: pve_client::types::VmEntry },
    },
)]
/// Query the remote's list of qemu VMs. If no node is provided, the all nodes are queried.
pub async fn list_qemu(
    remote: String,
    node: Option<String>,
) -> Result<Vec<pve_client::types::VmEntry>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = match get_remote(&remotes, &remote)? {
        Remote::Pve(pve) => connect(pve)?,
    };

    if let Some(node) = node {
        pve.list_qemu(&node, None).await
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
        items: { type: pve_client::types::VmEntry },
    },
)]
/// Query the remote's list of lxc containers. If no node is provided, the all nodes are queried.
pub async fn list_lxc(
    remote: String,
    node: Option<String>,
) -> Result<Vec<pve_client::types::LxcEntry>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = match get_remote(&remotes, &remote)? {
        Remote::Pve(pve) => connect(pve)?,
    };

    if let Some(node) = node {
        pve.list_lxc(&node).await
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
    returns: { type: pve_client::types::QemuConfig },
)]
/// Get the configuration of a qemu VM from a remote. If a node is provided, the VM must be on that
/// node, otherwise the node is determined automatically.
pub async fn qemu_get_config(
    remote: String,
    node: Option<String>,
    vmid: u64,
    state: ConfigurationState,
    snapshot: Option<String>,
) -> Result<pve_client::types::QemuConfig, Error> {
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

    pve.qemu_get_config(&node, vmid, state.current(), snapshot)
        .await
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
    returns: { type: pve_client::types::LxcConfig },
)]
/// Get the configuration of an lxc container from a remote. If a node is provided, the container
/// must be on that node, otherwise the node is determined automatically.
pub async fn lxc_get_config(
    remote: String,
    node: Option<String>,
    vmid: u64,
    state: ConfigurationState,
    snapshot: Option<String>,
) -> Result<pve_client::types::LxcConfig, Error> {
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

    pve.lxc_get_config(&node, vmid, state.current(), snapshot)
        .await
}
