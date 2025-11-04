use std::collections::HashSet;

use anyhow::{format_err, Error};
use pbs_api_types::REMOTE_ID_SCHEMA;
use pdm_api_types::{
    remotes::RemoteType,
    sdn::{CreateVnetRemote, ListVnet, SDN_ID_SCHEMA, VXLAN_ID_SCHEMA},
    Authid, PRIV_RESOURCE_AUDIT,
};
use proxmox_access_control::CachedUserInfo;
use proxmox_rest_server::WorkerTask;
use proxmox_router::{http_bail, Permission, Router, RpcEnvironment};
use proxmox_schema::api;
use pve_api_types::{CreateVnet, SdnVnetType};

use crate::{
    api::pve,
    parallel_fetcher::{NodeResults, ParallelFetcher},
    sdn_client::LockedSdnClients,
};

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_VNETS)
    .post(&API_METHOD_CREATE_VNET);

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
        description: "Get a list of controllers fitting the filtering criteria.",
        items: {
            type: ListVnet,
        },
    },
    access: {
        permission: &Permission::Anybody,
        description: "The user needs to have at least the `Resource.Audit` privilege under `/resource`.
        Only vnets from remotes for which the user has `Resource.Audit` on `/resource/{remote_name}`
        will be included in the returned list."
    }
)]
/// Query VNets of PVE remotes with optional filtering options
async fn list_vnets(
    pending: Option<bool>,
    running: Option<bool>,
    remotes: Option<HashSet<String>>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<ListVnet>, Error> {
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
        user_info.lookup_privs(&auth_id, &["resource", &remote_name]) & PRIV_RESOURCE_AUDIT != 0
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
    let fetcher = ParallelFetcher::new((pending, running));

    let results = fetcher
        .do_for_all_remotes(filtered_remotes, async |ctx, r, _| {
            Ok(pve::connect(&r)?.list_vnets(ctx.0, ctx.1).await?)
        })
        .await;

    for (remote, remote_result) in results.remote_results.into_iter() {
        match remote_result {
            Ok(remote_result) => {
                for (node, node_result) in remote_result.node_results.into_iter() {
                    match node_result {
                        Ok(NodeResults { data, .. }) => {
                            vnets.extend(data.into_iter().map(|vnet| ListVnet {
                                remote: remote.clone(),
                                vnet,
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
            vnet: { schema: SDN_ID_SCHEMA },
            tag: { schema: VXLAN_ID_SCHEMA, optional: true },
            remotes: {
                type: Array,
                description: "List of remotes with the zone in which the VNet should get created.",
                items: {
                    type: CreateVnetRemote,
                }
            },
        },
    },
    returns: { type: String, description: "Worker UPID" },
)]
/// Create a VNet across multiple remotes
async fn create_vnet(
    vnet: String,
    tag: Option<u32>,
    remotes: Vec<CreateVnetRemote>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<String, Error> {
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .ok_or_else(|| format_err!("no authid available"))?
        .parse()?;

    let upid = WorkerTask::spawn(
        "create_vnet",
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
                proxmox_log::info!("creating vnet {} on remote {}", vnet, ctx.remote_id());

                let params = CreateVnet {
                    alias: None,
                    isolate_ports: None,
                    tag,
                    ty: Some(SdnVnetType::Vnet),
                    vlanaware: None,
                    vnet: vnet.to_string(),
                    zone: ctx.data().zone.clone(),
                    lock_token: None,
                };

                client.create_vnet(params).await
            })
            .await?
            .apply_and_release()
            .await
        },
    )?;

    Ok(upid)
}
