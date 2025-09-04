use std::collections::HashSet;

use anyhow::{format_err, Error};

use pbs_api_types::REMOTE_ID_SCHEMA;
use pdm_api_types::{remotes::RemoteType, sdn::ListZone};
use proxmox_router::Router;
use proxmox_schema::api;
use pve_api_types::ListZonesType;

use crate::{
    api::pve,
    parallel_fetcher::{NodeResults, ParallelFetcher},
    sdn_client::LockedSdnClients,
};

pub const ROUTER: Router = Router::new().get(&API_METHOD_LIST_ZONES);

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
)]
/// Query zones of remotes with optional filtering options
pub async fn list_zones(
    pending: Option<bool>,
    running: Option<bool>,
    ty: Option<ListZonesType>,
    remotes: Option<HashSet<String>>,
) -> Result<Vec<ListZone>, Error> {
    let (remote_config, _) = pdm_config::remotes::config()?;

    let filtered_remotes = remote_config.into_iter().filter_map(|(_, remote)| {
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
