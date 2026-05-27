use std::collections::HashMap;

use anyhow::Error;
use futures::future::join_all;

use proxmox_schema::PropertyString;

use pdm_api_types::remotes::{Remote, RemoteType};
use pdm_api_types::{CachedLocationInfo, Location};
use pve_api_types::{NodeConfigProperty, PveNodeLocation};

use crate::{api_cache, connection};

const LOCATION_STATE_CACHE_KEY: &str = "location-state";

/// Get the location info from a remote, flow is as follows:
/// * try to get from the cache with `max_age` parameter
/// * if that fails, try to get updated location value
/// * if that fails try the cache again with `max_age` set to `u64::MAX`
///
/// This is done so we can get a location even if it was not cached within the last
/// day and the remote is e.g. offline or not reachable.
pub async fn get_location_info_for_remote(
    remote: &Remote,
    max_age: u64,
) -> Result<Option<CachedLocationInfo>, Error> {
    if let Some(cached) = get_cached_location_info(&remote.id, max_age).await? {
        Ok(Some(cached))
    } else {
        let location_info = match fetch_remote_location_info(remote).await {
            Ok(info) => info,
            Err(err) => {
                log::debug!(
                    "error getting location info for {}, falling back to cache: {err}",
                    remote.id
                );

                // last resort, if we can't get the location from here, we can't get any
                get_cached_location_info(&remote.id, u64::MAX).await?
            }
        };

        let now = proxmox_time::epoch_i64();

        if let Some(existing_state) =
            update_cached_location_info(&remote.id, location_info.clone(), now).await?
        {
            // Somebody else updated the cache while we performed the API request,
            // return the more recent data instead of the data we just fetched.
            return Ok(Some(existing_state));
        }
        Ok(location_info)
    }
}

async fn get_cached_location_info(
    remote: &str,
    max_age: u64,
) -> Result<Option<CachedLocationInfo>, Error> {
    let cache = api_cache::read_remote(remote).await?;
    // the cache uses an i64 internally and treats negative ages as expired, so clamp here to avoid
    // u64::MAX (our "unlimited" sentinel) wrapping to -1 and discarding every cached entry
    let max_age = max_age.min(i64::MAX as u64) as i64;
    let location_state = cache
        .get_with_max_age(LOCATION_STATE_CACHE_KEY, max_age)
        .await
        .inspect_err(|err| log::error!("could not read location-state from API cache: {err}"))
        .ok()
        .flatten();

    Ok(location_state)
}

async fn update_cached_location_info(
    remote: &str,
    info: Option<CachedLocationInfo>,
    now: i64,
) -> Result<Option<CachedLocationInfo>, Error> {
    let cache = api_cache::write_remote(remote).await?;

    Ok(cache
        .set_if_newer_with_timestamp(LOCATION_STATE_CACHE_KEY, info, now)
        .await?
        .flatten())
}

/// Parse a PVE location property string (from the datacenter or a node config) into our [Location].
fn parse_pve_location(value: Option<String>) -> Option<Location> {
    let location = value?
        .parse::<PropertyString<PveNodeLocation>>()
        .ok()?
        .into_inner();
    Some(Location {
        name: location.name,
        latitude: location.latitude,
        longitude: location.longitude,
    })
}

async fn fetch_remote_location_info(remote: &Remote) -> Result<Option<CachedLocationInfo>, Error> {
    match remote.ty {
        RemoteType::Pve => {
            let client = connection::make_pve_client(remote)?;

            // first, get the datacenter-wide default location
            let location = parse_pve_location(client.cluster_options().await?.location);

            // then get the individual node locations
            let mut node_locations = HashMap::new();
            let nodes = client.list_nodes().await?;
            let mut futures = Vec::with_capacity(nodes.len());
            for node in nodes.iter() {
                let future = client.node_config(&node.node, Some(NodeConfigProperty::Location));
                futures.push(async move { (node.node.clone(), future.await) });
            }

            for (node_name, remote_info) in join_all(futures).await {
                // don't fail the whole remote if a single node is unreachable, just fall back to
                // the datacenter location for that node below
                let node_config = match remote_info {
                    Ok(config) => Some(config),
                    Err(err) => {
                        log::debug!("could not get node config for '{node_name}': {err}");
                        None
                    }
                };
                let node_location =
                    parse_pve_location(node_config.and_then(|config| config.location));

                match (node_location, &location) {
                    (Some(location), _) => {
                        node_locations.insert(node_name, location);
                    }
                    (None, Some(location)) => {
                        node_locations.insert(node_name, location.clone());
                    }
                    _ => {}
                }
            }
            if node_locations.is_empty() {
                Ok(None)
            } else {
                Ok(Some(CachedLocationInfo { node_locations }))
            }
        }
        RemoteType::Pbs => {
            let client = connection::make_pbs_client(remote)?;
            let loc = client.node_config().await?.location.map(|location| {
                let location = location.into_inner();
                let mut node_locations = HashMap::new();
                node_locations.insert(
                    "localhost".to_string(),
                    Location {
                        name: location.name,
                        latitude: location.latitude,
                        longitude: location.longitude,
                    },
                );
                CachedLocationInfo { node_locations }
            });

            Ok(loc)
        }
    }
}
