use std::collections::HashSet;

use anyhow::{format_err, Error};

use pbs_api_types::REMOTE_ID_SCHEMA;
use pdm_api_types::{remotes::RemoteType, sdn::ListVnet};
use proxmox_router::Router;
use proxmox_schema::api;

use crate::{
    api::pve,
    parallel_fetcher::{NodeResults, ParallelFetcher},
    sdn_client::LockedSdnClients,
};

pub const ROUTER: Router = Router::new().get(&API_METHOD_LIST_VNETS);

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
)]
/// Query VNets of PVE remotes with optional filtering options
async fn list_vnets(
    pending: Option<bool>,
    running: Option<bool>,
    remotes: Option<HashSet<String>>,
) -> Result<Vec<ListVnet>, Error> {
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
