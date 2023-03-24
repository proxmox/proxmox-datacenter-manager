//! Manage PVE instances.

use anyhow::{format_err, Error};

use proxmox_client::Environment;
use proxmox_router::{list_subdirs_api_method, Router, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use pdm_api_types::{PveRemote, Remote, NODE_SCHEMA, REMOTE_ID_SCHEMA};

use super::remotes::get_remote;

pub const ROUTER: Router = Router::new().match_all("remote", &MAIN_ROUTER);

const MAIN_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([("nodes", &NODES_ROUTER), ("vms", &VMS_ROUTER)]);

const NODES_ROUTER: Router = Router::new().get(&API_METHOD_LIST_NODES);
const VMS_ROUTER: Router = Router::new().get(&API_METHOD_LIST_VMS);

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
/// Query the remote's version.
///
/// FIXME: Should we add an option to explicitly query the entire cluster to get a full version
/// overview?
pub async fn list_vms(
    remote: String,
    node: Option<String>,
) -> Result<Vec<pve_client::types::VmEntry>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = match get_remote(&remotes, &remote)? {
        Remote::Pve(pve) => connect(pve)?,
    };

    if let Some(node) = node {
        let x = pve.list_qemu(&node, None).await?;
        log::info!("GOT VM LIST: {}", x.len());
        for a in &x {
            log::info!("==> {}", a.vmid);
        }
        Ok(x)
    } else {
        let mut entry = Vec::new();
        for node in pve.list_nodes().await? {
            entry.extend(pve.list_qemu(&node.node, None).await?);
        }
        Ok(entry)
    }
}
