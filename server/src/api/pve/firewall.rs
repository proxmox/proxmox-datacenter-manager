use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Error;

use proxmox_router::{list_subdirs_api_method, Permission, Router, RpcEnvironment, SubdirMap};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;
use pve_api_types::{ClusterResource, ClusterResourceKind, ClusterResourceType};

use pdm_api_types::firewall::{
    FirewallStatus, GuestFirewallStatus, GuestKind, NodeFirewallStatus, RemoteFirewallStatus,
    RuleStat,
};
use pdm_api_types::remotes::{Remote, REMOTE_ID_SCHEMA};
use pdm_api_types::{NODE_SCHEMA, VMID_SCHEMA};
use pdm_api_types::{PRIV_RESOURCE_AUDIT, PRIV_RESOURCE_MODIFY, PRIV_SYS_MODIFY};

use super::{connect_to_remote, find_node_for_vm};
use crate::connection::PveClient;
use crate::parallel_fetcher::ParallelFetcher;

// top-level firewall routers
pub const PVE_FW_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(PVE_FW_SUBDIRS))
    .subdirs(PVE_FW_SUBDIRS);

pub const CLUSTER_FW_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(CLUSTER_FW_SUBDIRS))
    .subdirs(CLUSTER_FW_SUBDIRS);

pub const NODE_FW_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(NODE_FW_SUBDIRS))
    .subdirs(NODE_FW_SUBDIRS);

pub const LXC_FW_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(LXC_FW_SUBDIRS))
    .subdirs(LXC_FW_SUBDIRS);
pub const QEMU_FW_ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(QEMU_FW_SUBDIRS))
    .subdirs(QEMU_FW_SUBDIRS);

// pve
#[sortable]
const PVE_FW_SUBDIRS: SubdirMap = &sorted!([("status", &PVE_STATUS_ROUTER),]);

// cluster
#[sortable]
const CLUSTER_FW_SUBDIRS: SubdirMap = &sorted!([
    ("options", &CLUSTER_OPTIONS_ROUTER),
    ("rules", &CLUSTER_RULES_ROUTER),
    ("status", &CLUSTER_STATUS_ROUTER),
]);

// node
#[sortable]
const NODE_FW_SUBDIRS: SubdirMap = &sorted!([
    ("options", &NODE_OPTIONS_ROUTER),
    ("rules", &NODE_RULES_ROUTER),
    ("status", &NODE_STATUS_ROUTER),
]);

// guest
#[sortable]
const LXC_FW_SUBDIRS: SubdirMap = &sorted!([
    ("options", &LXC_OPTIONS_ROUTER),
    ("rules", &LXC_RULES_ROUTER),
]);
#[sortable]
const QEMU_FW_SUBDIRS: SubdirMap = &sorted!([
    ("options", &QEMU_OPTIONS_ROUTER),
    ("rules", &QEMU_RULES_ROUTER),
]);

// /options
const CLUSTER_OPTIONS_ROUTER: Router = Router::new()
    .get(&API_METHOD_CLUSTER_FIREWALL_OPTIONS)
    .put(&API_METHOD_UPDATE_CLUSTER_FIREWALL_OPTIONS);

const NODE_OPTIONS_ROUTER: Router = Router::new()
    .get(&API_METHOD_NODE_FIREWALL_OPTIONS)
    .put(&API_METHOD_UPDATE_NODE_FIREWALL_OPTIONS);

const LXC_OPTIONS_ROUTER: Router = Router::new()
    .get(&API_METHOD_LXC_FIREWALL_OPTIONS)
    .put(&API_METHOD_UPDATE_LXC_FIREWALL_OPTIONS);
const QEMU_OPTIONS_ROUTER: Router = Router::new()
    .get(&API_METHOD_QEMU_FIREWALL_OPTIONS)
    .put(&API_METHOD_UPDATE_QEMU_FIREWALL_OPTIONS);

// /rules
const CLUSTER_RULES_ROUTER: Router = Router::new().get(&API_METHOD_CLUSTER_FIREWALL_RULES);
const NODE_RULES_ROUTER: Router = Router::new().get(&API_METHOD_NODE_FIREWALL_RULES);
const LXC_RULES_ROUTER: Router = Router::new().get(&API_METHOD_LXC_FIREWALL_RULES);
const QEMU_RULES_ROUTER: Router = Router::new().get(&API_METHOD_QEMU_FIREWALL_RULES);

// /status
const PVE_STATUS_ROUTER: Router = Router::new().get(&API_METHOD_PVE_FIREWALL_STATUS);
const CLUSTER_STATUS_ROUTER: Router = Router::new().get(&API_METHOD_CLUSTER_FIREWALL_STATUS);
const NODE_STATUS_ROUTER: Router = Router::new().get(&API_METHOD_NODE_FIREWALL_STATUS);

#[derive(Clone)]
struct FirewallFetchContext {
    guests: Arc<Vec<ClusterResource>>,
}

#[derive(Clone, Debug)]
struct ClusterFirewallData {
    status: Option<FirewallStatus>,
    guests: Vec<ClusterResource>,
}

async fn fetch_cluster_firewall_data(
    _context: (),
    remote: Remote,
    _node: String, // unused for cluster-level data
) -> Result<ClusterFirewallData, Error> {
    let pve = crate::connection::make_pve_client(&remote)?;

    let guests = match pve.cluster_resources(Some(ClusterResourceKind::Vm)).await {
        Ok(guests) => guests,
        Err(_) => {
            return Ok(ClusterFirewallData {
                status: None,
                guests: vec![],
            });
        }
    };

    let options_response = pve.cluster_firewall_options();
    let rules_response = pve.list_cluster_firewall_rules();

    let enabled = options_response
        .await
        .map(|opts| opts.enable.is_some_and(|e| e != 0));
    let rules = rules_response.await.map(|rules| {
        let all = rules.len();
        let active = rules.iter().filter(|r| r.enable == Some(1)).count();
        RuleStat { all, active }
    });

    let status = match (enabled, rules) {
        (Ok(enabled), Ok(rules)) => Some(FirewallStatus { enabled, rules }),
        _ => None,
    };

    Ok(ClusterFirewallData { status, guests })
}

async fn load_guests_firewall_status(
    pve: Arc<PveClient>,
    node: String,
    guests: &[ClusterResource],
) -> Vec<GuestFirewallStatus> {
    let mut result = vec![];

    let guests: Vec<(u32, String, GuestKind)> = guests
        .iter()
        .filter(|g| g.node.as_ref() == Some(&node))
        .filter_map(|g| {
            let vmid = g.vmid?;
            let name = g.name.clone().unwrap_or_default();
            match g.ty {
                ClusterResourceType::Lxc => Some((vmid, name, GuestKind::Lxc)),
                ClusterResourceType::Qemu => Some((vmid, name, GuestKind::Qemu)),
                _ => None,
            }
        })
        .collect();

    for (vmid, name, kind) in guests {
        let options_response = match kind {
            GuestKind::Lxc => pve.lxc_firewall_options(&node, vmid),
            GuestKind::Qemu => pve.qemu_firewall_options(&node, vmid),
        };
        let rules_response = match kind {
            GuestKind::Lxc => pve.list_lxc_firewall_rules(&node, vmid),
            GuestKind::Qemu => pve.list_qemu_firewall_rules(&node, vmid),
        };

        let enabled = options_response
            .await
            .map(|opts| opts.enable.unwrap_or_default());
        let rules = rules_response.await.map(|rules| {
            let all = rules.len();
            let active = rules.iter().filter(|r| r.enable == Some(1)).count();
            RuleStat { all, active }
        });

        let status = match (enabled, rules) {
            (Ok(enabled), Ok(rules)) => Some(FirewallStatus { enabled, rules }),
            _ => None,
        };

        result.push(GuestFirewallStatus {
            vmid,
            name,
            status,
            kind,
        });
    }
    result
}

async fn fetch_node_firewall_status(
    context: FirewallFetchContext,
    remote: Remote,
    node: String,
) -> Result<NodeFirewallStatus, Error> {
    let pve = crate::connection::make_pve_client(&remote)?;

    let options_response = pve.node_firewall_options(&node);
    let rules_response = pve.list_node_firewall_rules(&node);

    let enabled = options_response
        .await
        .map(|opts| opts.enable.unwrap_or(true));
    let rules = rules_response.await.map(|rules| {
        let all = rules.len();
        let active = rules.iter().filter(|r| r.enable == Some(1)).count();
        RuleStat { all, active }
    });

    let status = match (enabled, rules) {
        (Ok(enabled), Ok(rules)) => Some(FirewallStatus { enabled, rules }),
        _ => None,
    };

    let guests_status = load_guests_firewall_status(pve, node.clone(), &context.guests).await;

    Ok(NodeFirewallStatus {
        node,
        status,
        guests: guests_status,
    })
}

#[api(
    returns: {
        type: Array,
        description: "Get firewall status of remotes",
        items: { type: RemoteFirewallStatus },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get firewall status of all PVE remotes.
pub async fn pve_firewall_status(
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<RemoteFirewallStatus>, Error> {
    let (remote_config, _) = pdm_config::remotes::config()?;

    let pve_remotes: Vec<Remote> = remote_config
        .into_iter()
        .filter_map(|(_, remote)| match remote.ty {
            pdm_api_types::remotes::RemoteType::Pve => Some(remote),
            pdm_api_types::remotes::RemoteType::Pbs => None,
        })
        .collect();

    if pve_remotes.is_empty() {
        return Ok(vec![]);
    }

    // 1: fetch cluster-level data (status + guests)
    let cluster_fetcher = ParallelFetcher::new(());
    let cluster_results = cluster_fetcher
        .do_for_all_remotes(pve_remotes.iter().cloned(), fetch_cluster_firewall_data)
        .await;

    // 2: build context with guests for each remote and fetch node-level data
    let mut guests_per_remote = HashMap::new();
    for (remote_id, remote_result) in &cluster_results.remote_results {
        if let Ok(remote_result) = remote_result {
            if let Ok(node_result) = remote_result.node_results.get("localhost").unwrap() {
                guests_per_remote
                    .insert(remote_id.clone(), Arc::new(node_result.data.guests.clone()));
            }
        }
    }

    let context = FirewallFetchContext {
        guests: Arc::new(vec![]),
    };

    let node_fetcher = ParallelFetcher::new(context);
    let node_results = node_fetcher
        .do_for_all_remote_nodes(pve_remotes.iter().cloned(), move |mut ctx, remote, node| {
            if let Some(guests) = guests_per_remote.get(&remote.id) {
                ctx.guests = guests.clone();
            }
            fetch_node_firewall_status(ctx, remote, node)
        })
        .await;

    // 3: combine results
    let mut result = Vec::new();
    for remote in &pve_remotes {
        let mut cluster_status = cluster_results
            .remote_results
            .get(&remote.id)
            .and_then(|r| r.as_ref().ok())
            .and_then(|r| r.node_results.get("localhost"))
            .and_then(|n| n.as_ref().ok())
            .and_then(|n| n.data.status.clone());

        let node_fetch_result = node_results.remote_results.get(&remote.id);

        let nodes = node_fetch_result
            .and_then(|r| r.as_ref().ok())
            .map(|r| {
                r.node_results
                    .values()
                    .filter_map(|n| n.as_ref().ok().map(|n| n.data.clone()))
                    .collect()
            })
            .unwrap_or_default();

        if node_fetch_result.and_then(|r| r.as_ref().err()).is_some() {
            cluster_status = None;
        }

        result.push(RemoteFirewallStatus {
            remote: remote.id.clone(),
            status: cluster_status,
            nodes,
        });
    }

    Ok(result)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
        },
    },
    returns: { type: pve_api_types::ClusterFirewallOptions },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get cluster firewall options.
pub async fn cluster_firewall_options(
    remote: String,
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<pve_api_types::ClusterFirewallOptions, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let pve = connect_to_remote(&remotes, &remote)?;

    Ok(pve.cluster_firewall_options().await?)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
        },
    },
    returns: {
        type: RemoteFirewallStatus,
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get firewall status of a specific remote.
pub async fn cluster_firewall_status(
    remote: String,
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<RemoteFirewallStatus, Error> {
    let (remote_config, _) = pdm_config::remotes::config()?;

    let remote_obj = remote_config
        .into_iter()
        .find(|(id, _)| id == &remote)
        .map(|(_, r)| r)
        .ok_or_else(|| anyhow::format_err!("Remote '{}' not found", remote))?;

    // 1: fetch cluster-level data (status + guests)
    let cluster_fetcher = ParallelFetcher::new(());
    let cluster_results = cluster_fetcher
        .do_for_all_remotes(
            std::iter::once(remote_obj.clone()),
            fetch_cluster_firewall_data,
        )
        .await;

    let cluster_data = cluster_results
        .remote_results
        .get(&remote)
        .and_then(|r| r.as_ref().ok())
        .and_then(|r| r.node_results.get("localhost"))
        .and_then(|n| n.as_ref().ok())
        .map(|n| &n.data);

    let (cluster_status, guests) = match cluster_data {
        Some(data) => (data.status.clone(), data.guests.clone()),
        None => {
            return Ok(RemoteFirewallStatus {
                remote,
                status: None,
                nodes: vec![],
            });
        }
    };

    // 2: fetch node-level data
    let context = FirewallFetchContext {
        guests: Arc::new(guests),
    };

    let node_fetcher = ParallelFetcher::new(context);
    let node_results = node_fetcher
        .do_for_all_remote_nodes(std::iter::once(remote_obj), fetch_node_firewall_status)
        .await;

    // 3: collect node results
    let node_fetch_result = node_results.remote_results.get(&remote);

    let nodes = node_fetch_result
        .and_then(|r| r.as_ref().ok())
        .map(|r| {
            r.node_results
                .values()
                .filter_map(|n| n.as_ref().ok().map(|n| n.data.clone()))
                .collect()
        })
        .unwrap_or_default();

    let final_status = if node_fetch_result.and_then(|r| r.as_ref().err()).is_some() {
        None
    } else {
        cluster_status
    };

    Ok(RemoteFirewallStatus {
        remote,
        status: final_status,
        nodes,
    })
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
            },
        },
    },
    returns: { type: pve_api_types::NodeFirewallOptions },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get nodes firewall options.
pub async fn node_firewall_options(
    remote: String,
    node: String,
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<pve_api_types::NodeFirewallOptions, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let pve = connect_to_remote(&remotes, &remote)?;

    Ok(pve.node_firewall_options(&node).await?)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: { schema: NODE_SCHEMA },
        },
    },
    returns: {
        type: NodeFirewallStatus,
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get firewall status of a specific node.
pub async fn node_firewall_status(
    remote: String,
    node: String,
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<NodeFirewallStatus, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let pve = connect_to_remote(&remotes, &remote)?;

    let guests = pve.cluster_resources(Some(ClusterResourceKind::Vm)).await?;

    let options_response = pve.node_firewall_options(&node);
    let rules_response = pve.list_node_firewall_rules(&node);

    let enabled = options_response
        .await
        .map(|opts| opts.enable.unwrap_or_default());
    let rules = rules_response.await.map(|rules| {
        let all = rules.len();
        let active = rules.iter().filter(|r| r.enable == Some(1)).count();
        RuleStat { all, active }
    });

    let status = match (enabled, rules) {
        (Ok(enabled), Ok(rules)) => Some(FirewallStatus { enabled, rules }),
        _ => None,
    };

    let guests_status = load_guests_firewall_status(pve, node.clone(), &guests).await;

    Ok(NodeFirewallStatus {
        node,
        status,
        guests: guests_status,
    })
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
        },
    },
    returns: {
        type: Array,
        description: "List cluster firewall rules.",
        items: { type: pve_api_types::ListFirewallRules },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get cluster firewall rules.
pub async fn cluster_firewall_rules(
    remote: String,
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<pve_api_types::ListFirewallRules>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let pve = connect_to_remote(&remotes, &remote)?;

    Ok(pve.list_cluster_firewall_rules().await?)
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
    returns: { type: pve_api_types::GuestFirewallOptions },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get LXC firewall options.
pub async fn lxc_firewall_options(
    remote: String,
    node: Option<String>,
    vmid: u32,
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<pve_api_types::GuestFirewallOptions, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    Ok(pve.lxc_firewall_options(&node, vmid).await?)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            update: {
                type: pve_api_types::UpdateClusterFirewallOptions,
                flatten: true,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_SYS_MODIFY, false),
    },
)]
/// Update cluster firewall configuration
pub async fn update_cluster_firewall_options(
    remote: String,
    update: pve_api_types::UpdateClusterFirewallOptions,
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<(), Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let pve = connect_to_remote(&remotes, &remote)?;

    Ok(pve.set_cluster_firewall_options(update).await?)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
            },
            update: {
                type: pve_api_types::UpdateNodeFirewallOptions,
                flatten: true,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_MODIFY, false),
    },
)]
/// Update a nodes firewall configuration
pub async fn update_node_firewall_options(
    remote: String,
    node: String,
    update: pve_api_types::UpdateNodeFirewallOptions,
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<(), Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let pve = connect_to_remote(&remotes, &remote)?;

    Ok(pve.set_node_firewall_options(&node, update).await?)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
            },
        },
    },
    returns: {
        type: Array,
        description: "List node firewall rules.",
        items: { type: pve_api_types::ListFirewallRules },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get node firewall rules.
pub async fn node_firewall_rules(
    remote: String,
    node: String,
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<pve_api_types::ListFirewallRules>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;
    let pve = connect_to_remote(&remotes, &remote)?;

    Ok(pve.list_node_firewall_rules(&node).await?)
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
    returns: { type: pve_api_types::GuestFirewallOptions },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get QEMU firewall options.
pub async fn qemu_firewall_options(
    remote: String,
    node: Option<String>,
    vmid: u32,
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<pve_api_types::GuestFirewallOptions, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    Ok(pve.qemu_firewall_options(&node, vmid).await?)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA, },
            update: {
                type: pve_api_types::UpdateGuestFirewallOptions,
                flatten: true,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_MODIFY, false),
    },
)]
/// Update LXC firewall options
pub async fn update_lxc_firewall_options(
    remote: String,
    node: Option<String>,
    vmid: u32,
    update: pve_api_types::UpdateGuestFirewallOptions,
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<(), Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    Ok(pve.set_lxc_firewall_options(&node, vmid, update).await?)
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA, },
            update: {
                type: pve_api_types::UpdateGuestFirewallOptions,
                flatten: true,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_MODIFY, false),
    },
)]
/// Update QEMU firewall options
pub async fn update_qemu_firewall_options(
    remote: String,
    node: Option<String>,
    vmid: u32,
    update: pve_api_types::UpdateGuestFirewallOptions,
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<(), Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    Ok(pve.set_qemu_firewall_options(&node, vmid, update).await?)
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
        description: "List LXC firewall rules.",
        items: { type: pve_api_types::ListFirewallRules },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get LXC firewall rules.
pub async fn lxc_firewall_rules(
    remote: String,
    node: Option<String>,
    vmid: u32,
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<pve_api_types::ListFirewallRules>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    Ok(pve.list_lxc_firewall_rules(&node, vmid).await?)
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
        description: "List QEMU firewall rules.",
        items: { type: pve_api_types::ListFirewallRules },
    },
    access: {
        permission: &Permission::Privilege(&["resource", "{remote}", "guest", "{vmid}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get QEMU firewall rules.
pub async fn qemu_firewall_rules(
    remote: String,
    node: Option<String>,
    vmid: u32,
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<pve_api_types::ListFirewallRules>, Error> {
    let (remotes, _) = pdm_config::remotes::config()?;

    let pve = connect_to_remote(&remotes, &remote)?;

    let node = find_node_for_vm(node, vmid, pve.as_ref()).await?;

    Ok(pve.list_qemu_firewall_rules(&node, vmid).await?)
}
