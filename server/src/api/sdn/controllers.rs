use std::collections::HashSet;

use anyhow::{format_err, Error};

use pbs_api_types::REMOTE_ID_SCHEMA;
use pdm_api_types::{remotes::RemoteType, sdn::ListController, Authid, PRIV_RESOURCE_AUDIT};
use proxmox_access_control::CachedUserInfo;
use proxmox_router::{http_bail, Permission, Router, RpcEnvironment};
use proxmox_schema::api;
use pve_api_types::ListControllersType;

use crate::{
    api::pve,
    parallel_fetcher::{NodeResults, ParallelFetcher},
};

pub const ROUTER: Router = Router::new().get(&API_METHOD_LIST_CONTROLLERS);

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
                type: ListControllersType,
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
        description: "Get a list of controllers fitting the filtering criteria.",
        items: {
            type: ListController,
        },
    },
    access: {
        permission: &Permission::Anybody,
        description: "The user needs to have at least the `Resource.Audit` privilege under `/resource`.
        Only controllers from remotes for which the user has `Resource.Audit` on `/resource/{remote_name}`
        will be included in the returned list."
    }
)]
/// Query controllers of remotes with optional filtering options
pub async fn list_controllers(
    pending: Option<bool>,
    running: Option<bool>,
    ty: Option<ListControllersType>,
    remotes: Option<HashSet<String>>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<ListController>, Error> {
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
            Ok(pve::connect(&r)?
                .list_controllers(ctx.0, ctx.1, ctx.2)
                .await?)
        })
        .await;

    for (remote, remote_result) in results.remote_results.into_iter() {
        match remote_result {
            Ok(remote_result) => {
                for (node, node_result) in remote_result.node_results.into_iter() {
                    match node_result {
                        Ok(NodeResults { data, .. }) => {
                            vnets.extend(data.into_iter().map(|controller| ListController {
                                remote: remote.clone(),
                                controller,
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
