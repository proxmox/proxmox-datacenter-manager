use std::collections::HashMap;

use anyhow::{bail, Error};

use proxmox_router::{Permission, Router};
use proxmox_schema::api;
use proxmox_schema::api_types::NODE_SCHEMA;
use proxmox_subscription::files::update_apt_auth;
use proxmox_subscription::{SubscriptionInfo, SubscriptionStatus};
use proxmox_sys::fs::CreateOptions;

use pdm_api_types::remotes::RemoteType;
use pdm_api_types::subscription::{
    NodeSubscriptionInfo, PdmSubscriptionInfo, SubscriptionLevel, SubscriptionStatistics,
};
use pdm_api_types::PRIV_SYS_MODIFY;

use crate::api::resources::get_subscription_info_for_remote;

const PRODUCT_URL: &str = "https://pdm.proxmox.com/faq.html";
const APT_AUTH_FN: &str = "/etc/apt/auth.conf.d/pdm.conf";
const APT_AUTH_URL: &str = "enterprise.proxmox.com/debian/pdm";

// minimum ratio of nodes with active subscriptions
const SUBSCRIPTION_THRESHOLD: f64 = 0.8;

fn apt_auth_file_opts() -> CreateOptions {
    let mode = nix::sys::stat::Mode::from_bits_truncate(0o0600);
    CreateOptions::new().perm(mode).owner(nix::unistd::ROOT)
}

async fn get_all_subscription_infos(
) -> Result<HashMap<String, (RemoteType, HashMap<String, Option<NodeSubscriptionInfo>>)>, Error> {
    let (remotes_config, _digest) = pdm_config::remotes::config()?;

    let mut subscription_info = HashMap::new();
    for (remote_name, remote) in remotes_config.iter() {
        match get_subscription_info_for_remote(remote, 24 * 60 * 60).await {
            Ok(info) => {
                subscription_info.insert(remote_name.to_string(), (remote.ty, info));
            }
            Err(err) => {
                log::debug!("Failed to get subscription info for remote {remote_name} - {err}");
                subscription_info.insert(remote_name.to_string(), (remote.ty, HashMap::new()));
            }
        }
    }
    Ok(subscription_info)
}

fn count_subscriptions(
    subscription_infos: &HashMap<
        String,
        (RemoteType, HashMap<String, Option<NodeSubscriptionInfo>>),
    >,
) -> SubscriptionStatistics {
    let mut stats = SubscriptionStatistics::default();
    for (_remote, (_remote_type, remote_infos)) in subscription_infos.iter() {
        if remote_infos.is_empty() {
            // count remotes without info as at least one node
            stats.total_nodes += 1;
            continue;
        }
        for (_node, node_info) in remote_infos.iter() {
            stats.total_nodes += 1;
            if let Some(info) = node_info {
                if info.status == SubscriptionStatus::Active {
                    stats.active_subscriptions += 1;
                    if info.level == SubscriptionLevel::Community {
                        stats.community += 1;
                    }
                }
            }
        }
    }
    stats
}

fn check_counts(stats: &SubscriptionStatistics) -> Result<(), Error> {
    if stats.total_nodes == 0 {
        bail!("No remotes configured or could not fetch status.");
    }
    let basic_or_higher_ratio =
        (stats.active_subscriptions - stats.community) as f64 / stats.total_nodes as f64;

    if basic_or_higher_ratio >= SUBSCRIPTION_THRESHOLD {
        return Ok(());
    } else {
        bail!("Too many remote nodes without active basic or higher subscription!");
    }
}

#[api(
    access: { permission: &Permission::Anybody, },
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
        },
    },
    returns: {
        type: SubscriptionInfo,
    }
)]
/// Return subscription status
pub async fn get_subscription() -> Result<PdmSubscriptionInfo, Error> {
    let infos = get_all_subscription_infos().await?;

    let statistics = count_subscriptions(&infos);

    let info = if let Err(err) = check_counts(&statistics) {
        SubscriptionInfo {
            status: SubscriptionStatus::Invalid,
            message: Some(format!("{err}")),
            serverid: None,
            url: Some(PRODUCT_URL.into()),
            ..Default::default()
        }
    } else {
        SubscriptionInfo {
            status: SubscriptionStatus::Active,
            url: Some(PRODUCT_URL.into()),
            ..Default::default()
        }
    };

    Ok(PdmSubscriptionInfo { info, statistics })
}

#[api(
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
        },
    },
    protected: true,
    access: {
        permission: &Permission::Privilege(&["system"], PRIV_SYS_MODIFY, false),
    },
)]
/// Update subscription information
pub async fn check_subscription() -> Result<(), Error> {
    let infos = get_all_subscription_infos().await?;
    let stats = count_subscriptions(&infos);

    if let Err(err) = check_counts(&stats) {
        update_apt_auth(APT_AUTH_FN, apt_auth_file_opts(), APT_AUTH_URL, None, None)?;
        log::debug!("failed PDM subscription eligibility check - {err}");
        return Err(err);
    }

    let mut found = false;
    'outer: for (remote, (remote_type, remote_info)) in infos.iter() {
        if *remote_type != RemoteType::Pve && *remote_type != RemoteType::Pbs {
            log::warn!("skipping unknown remote type {remote_type}");
            continue;
        }
        for (node, node_info) in remote_info.iter() {
            if let Some(info) = node_info {
                if info.status == SubscriptionStatus::Active
                    && info.level >= SubscriptionLevel::Basic
                    && info.key.is_some()
                    && info.serverid.is_some()
                {
                    log::info!("Using subscription of node '{node}' of remote '{remote}' for enterprise repository access");
                    update_apt_auth(
                        APT_AUTH_FN,
                        apt_auth_file_opts(),
                        APT_AUTH_URL,
                        info.key.clone(),
                        info.serverid.clone(),
                    )?;
                    found = true;
                    break 'outer;
                }
            } else {
                log::debug!("no subscription info found for {node}");
            }
        }
    }

    if !found {
        log::warn!(
            "No valid Basic+ subscription found for configuring enterprise repository access.."
        );
        update_apt_auth(APT_AUTH_FN, apt_auth_file_opts(), APT_AUTH_URL, None, None)?;
    }

    Ok(())
}

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_GET_SUBSCRIPTION)
    .post(&API_METHOD_CHECK_SUBSCRIPTION);
