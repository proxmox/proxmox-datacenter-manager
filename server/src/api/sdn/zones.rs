use std::collections::HashSet;

use anyhow::{format_err, Error};

use pbs_api_types::REMOTE_ID_SCHEMA;
use pdm_api_types::{
    remotes::RemoteType,
    sdn::{CreateZoneRemote, ListZone, SDN_ID_SCHEMA, VXLAN_ID_SCHEMA},
    Authid, PRIV_RESOURCE_AUDIT,
};
use proxmox_access_control::CachedUserInfo;
use proxmox_rest_server::WorkerTask;
use proxmox_router::{http_bail, Permission, Router, RpcEnvironment};
use proxmox_schema::api;
use pve_api_types::{CreateZone, ListZonesType};

use crate::{
    api::pve,
    parallel_fetcher::{NodeResults, ParallelFetcher},
    sdn_client::LockedSdnClients,
};

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_ZONES)
    .post(&API_METHOD_CREATE_ZONE);

#[api(
    input: {
        properties: {
            pending: {
                type: Boolean,
                optional: true,
                description: "Include a list of attributes whose changes are currently pending.",
            },
            running: {
                type: Boolean,
                optional: true,
                description: "If true shows the running configuration, otherwise the pending configuration.",
            },
            ty: {
                type: ListZonesType,
                optional: true,
            },
            remotes: {
                type: Array,
                optional: true,
                description: "Only return controllers from the specified remotes.",
                items: {
                    schema: REMOTE_ID_SCHEMA,
                }
            },
        }
    },
    returns: {
        type: Array,
        description: "Get a list of zones fitting the filtering criteria.",
        items: {
            type: ListZone,
        },
    },
    access: {
        permission: &Permission::Anybody,
        description: "The user needs to have at least the `Resource.Audit` privilege under `/resource`.
        Only zones from remotes for which the user has `Resource.Audit` on `/resource/{remote_name}`
        will be included in the returned list."
    }
)]
/// Query zones of remotes with optional filtering options
pub async fn list_zones(
    pending: Option<bool>,
    running: Option<bool>,
    ty: Option<ListZonesType>,
    remotes: Option<HashSet<String>>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<ListZone>, Error> {
    let user_info = CachedUserInfo::new()?;

    let auth_id: Authid = rpcenv
        .get_auth_id()
        .ok_or_else(|| format_err!("no authid available"))?
        .parse()?;

    if !user_info.any_privs_below(&auth_id, &["resource"], PRIV_RESOURCE_AUDIT)? {
        http_bail!(FORBIDDEN, "user has no access to resources");
    }

    let (remote_config, _) = pdm_config::remotes::config()?;
    let authorized_remotes = remote_config.into_iter().filter(|(remote_name, _)| {
        user_info.lookup_privs(&auth_id, &["resource", remote_name]) & PRIV_RESOURCE_AUDIT != 0
    });

    let filtered_remotes = authorized_remotes.filter_map(|(_, remote)| {
        if remote.ty == RemoteType::Pve
            && remotes
                .as_ref()
                .map(|remotes| remotes.contains(&remote.id))
                .unwrap_or(true)
        {
            return Some(remote);
        }

        None
    });

    let mut vnets = Vec::new();
    let fetcher = ParallelFetcher::new((pending, running, ty));

    let results = fetcher
        .do_for_all_remotes(filtered_remotes, async |ctx, r, _| {
            Ok(pve::connect(&r)?.list_zones(ctx.0, ctx.1, ctx.2).await?)
        })
        .await;

    for (remote, remote_result) in results.remote_results.into_iter() {
        match remote_result {
            Ok(remote_result) => {
                for (node, node_result) in remote_result.node_results.into_iter() {
                    match node_result {
                        Ok(NodeResults { data, .. }) => {
                            vnets.extend(data.into_iter().map(|zone| ListZone {
                                remote: remote.clone(),
                                zone,
                            }))
                        }
                        Err(error) => {
                            log::error!(
                                "could not fetch vnets from remote {} node {}: {error:#}",
                                remote,
                                node
                            );
                        }
                    }
                }
            }
            Err(error) => {
                log::error!("could not fetch vnets from remote {}: {error:#}", remote)
            }
        }
    }

    Ok(vnets)
}

#[api(
    input: {
        properties: {
            zone: { schema: SDN_ID_SCHEMA },
            "vrf-vxlan": {
                schema: VXLAN_ID_SCHEMA,
                optional: true,
            },
            remotes: {
                type: Array,
                description: "List of remotes with their controller where zone should get created.",
                items: {
                    type: CreateZoneRemote
                }
            },
        },
    },
    returns: { type: String, description: "Worker UPID" },
)]
/// Create a zone across multiple remotes
async fn create_zone(
    zone: String,
    vrf_vxlan: Option<u32>,
    remotes: Vec<CreateZoneRemote>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<String, Error> {
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .ok_or_else(|| format_err!("no authid available"))?
        .parse()?;

    let upid = WorkerTask::spawn(
        "create_zone",
        None,
        auth_id.to_string(),
        false,
        move |_worker| async move {
            LockedSdnClients::from_remote_names(
                remotes
                    .into_iter()
                    .map(|remote| (remote.remote.clone(), remote)),
                false,
            )
            .await?
            .for_each(async move |client, ctx| {
                let params = CreateZone {
                    zone: zone.clone(),
                    vrf_vxlan,
                    controller: Some(ctx.data().controller.clone()),
                    ty: ListZonesType::Evpn,
                    advertise_subnets: None,
                    bridge: None,
                    bridge_disable_mac_learning: None,
                    dhcp: None,
                    disable_arp_nd_suppression: None,
                    dns: None,
                    dnszone: None,
                    dp_id: None,
                    exitnodes: None,
                    exitnodes_local_routing: None,
                    exitnodes_primary: None,
                    ipam: None,
                    mac: None,
                    mtu: None,
                    nodes: None,
                    peers: None,
                    reversedns: None,
                    rt_import: None,
                    tag: None,
                    vlan_protocol: None,
                    vxlan_port: None,
                    lock_token: None,
                    fabric: None,
                };

                client.create_zone(params).await
            })
            .await?
            .apply_and_release()
            .await
        },
    )?;

    Ok(upid)
}
