use anyhow::Error;

use proxmox_config_digest::PROXMOX_CONFIG_DIGEST_SCHEMA;
use proxmox_router::cli::{
    CliCommand, CliCommandMap, CommandLineInterface, OutputFormat, format_and_print_result,
};
use proxmox_schema::api;

use pdm_api_types::remotes::REMOTE_ID_SCHEMA;
use pdm_api_types::subscription::{RemoteSubscriptionState, SUBSCRIPTION_KEY_SCHEMA};
use pdm_api_types::{NODE_SCHEMA, VIEW_ID_SCHEMA};
use pdm_client::ConfigDigest;

use crate::env::emoji;
use crate::{client, env};

pub fn cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert(
            "status",
            CliCommand::new(&API_METHOD_GET_SUBSCRIPTION_STATUS),
        )
        .insert("list-keys", CliCommand::new(&API_METHOD_LIST_KEYS))
        .insert(
            "add-keys",
            CliCommand::new(&API_METHOD_ADD_KEYS).arg_param(&["keys"]),
        )
        .insert(
            "assign-key",
            CliCommand::new(&API_METHOD_ASSIGN_KEY).arg_param(&["key"]),
        )
        .insert(
            "clear-assignment",
            CliCommand::new(&API_METHOD_CLEAR_ASSIGNMENT).arg_param(&["key"]),
        )
        .insert(
            "remove-key",
            CliCommand::new(&API_METHOD_REMOVE_KEY).arg_param(&["key"]),
        )
        .insert("auto-assign", CliCommand::new(&API_METHOD_AUTO_ASSIGN))
        .insert("apply-pending", CliCommand::new(&API_METHOD_APPLY_PENDING))
        .insert("clear-pending", CliCommand::new(&API_METHOD_CLEAR_PENDING))
        .insert(
            "clear-key",
            CliCommand::new(&API_METHOD_CLEAR_KEY).arg_param(&["remote", "node"]),
        )
        .insert(
            "revert-clear",
            CliCommand::new(&API_METHOD_REVERT_CLEAR).arg_param(&["remote", "node"]),
        )
        .insert(
            "adopt-key",
            CliCommand::new(&API_METHOD_ADOPT_KEY).arg_param(&["remote", "node"]),
        )
        .insert("adopt-all", CliCommand::new(&API_METHOD_ADOPT_ALL))
        .insert(
            "check",
            CliCommand::new(&API_METHOD_CHECK_SUBSCRIPTION).arg_param(&["remote", "node"]),
        )
        .into()
}

#[api(
    input: {
        properties: {
            "max-age": {
                description: "Maximum age (in seconds) of cached remote subscription state.",
                // long default to not query it too often
                default: 24*60*60,
                optional: true,
            },
            verbose: {
                type: bool,
                optional: true,
                default: false,
                description: "If true, includes subscription information per node (with enough privileges)",
            },
            view: {
                schema: VIEW_ID_SCHEMA,
                optional: true,
            },
        },
    }
)]
/// Show the subscription status of all remotes.
async fn get_subscription_status(
    max_age: Option<u64>,
    verbose: Option<bool>,
    view: Option<String>,
) -> Result<(), Error> {
    let mut subscriptions = client()?
        .get_subscription_status(max_age, verbose, view.as_deref())
        .await?;
    let output_format = env().format_args.output_format;
    if output_format == OutputFormat::Text {
        if subscriptions.is_empty() {
            println!("No subscriptions.");
            return Ok(());
        }

        subscriptions.sort_by(|a, b| a.remote.cmp(&b.remote));
        let remote_width = subscriptions
            .iter()
            .map(|sub| sub.remote.len())
            .max()
            .unwrap_or(5);
        for entry in subscriptions {
            let mut symbol = "⚠️ ";
            let state = match entry.state {
                RemoteSubscriptionState::None => "at least one node with no subscription",
                RemoteSubscriptionState::Unknown => "status could not be determined",
                RemoteSubscriptionState::Mixed => "different subscription levels found",
                RemoteSubscriptionState::Active => {
                    symbol = "✅️ ";
                    "valid"
                }
            };
            let symbol = emoji(symbol);
            println!(
                "{symbol}{remote:width$} {state}",
                remote = format!("{}:", entry.remote),
                width = remote_width + 1,
            );

            if let Some(err) = entry.error {
                println!("    Errors querying the remote: {err}");
            }

            let Some(node_status) = entry.node_status else {
                continue;
            };

            let mut node_status = node_status.iter().collect::<Vec<_>>();

            node_status.sort_by(|a, b| a.0.cmp(b.0));
            for (node, info) in node_status {
                let Some(info) = info else {
                    println!("    {node}");
                    continue;
                };

                print!("    {node}: {} {}", info.status, info.level);
                if let Some(sockets) = info.sockets {
                    print!(" [{sockets} sockets]");
                }
                println!();
            }
        }
    } else {
        format_and_print_result(&subscriptions, &output_format.to_string());
    }
    Ok(())
}

#[api]
/// List all subscription keys in the pool.
async fn list_keys() -> Result<(), Error> {
    let (keys, _digest) = client()?.list_subscription_keys().await?;

    let output_format = env().format_args.output_format;
    if output_format == OutputFormat::Text {
        if keys.is_empty() {
            println!("No keys in pool.");
            return Ok(());
        }
        let key_width = keys.iter().map(|k| k.key.len()).max().unwrap_or(20);
        for key in &keys {
            let assignment = match (&key.remote, &key.node) {
                (Some(r), Some(n)) if key.pending_clear => {
                    format!("{r}/{n} [clear queued]")
                }
                (Some(r), Some(n)) => format!("{r}/{n}"),
                _ => "(unassigned)".to_string(),
            };
            println!(
                "  {key:<kw$}  {product:<5}  {level:<10}  {status:<10}  {assignment}",
                key = key.key,
                kw = key_width,
                product = key.product_type.to_string(),
                level = key.level.to_string(),
                status = key.status.to_string(),
            );
        }
    } else {
        format_and_print_result(&keys, &output_format.to_string());
    }
    Ok(())
}

#[api(
    input: {
        properties: {
            keys: {
                type: Array,
                description: "Subscription keys to add to the pool.",
                items: { schema: SUBSCRIPTION_KEY_SCHEMA },
            },
            digest: {
                schema: PROXMOX_CONFIG_DIGEST_SCHEMA,
                optional: true,
            },
        },
    },
)]
/// Add one or more subscription keys to the pool.
async fn add_keys(keys: Vec<String>, digest: Option<String>) -> Result<(), Error> {
    let digest = digest.map(ConfigDigest::from);
    let result = client()?.add_subscription_keys(&keys, digest).await?;
    if result.added == 1 {
        println!("Added 1 key to pool.");
    } else {
        println!("Added {} keys to pool.", result.added);
    }
    if result.deduplicated > 0 {
        println!(
            "Dropped {} duplicate key(s) from the input.",
            result.deduplicated
        );
    }
    Ok(())
}

#[api(
    input: {
        properties: {
            key: { schema: SUBSCRIPTION_KEY_SCHEMA },
            remote: { schema: REMOTE_ID_SCHEMA },
            node: { schema: NODE_SCHEMA },
            digest: {
                schema: PROXMOX_CONFIG_DIGEST_SCHEMA,
                optional: true,
            },
        },
    },
)]
/// Assign a key from the pool to a remote node.
async fn assign_key(
    key: String,
    remote: String,
    node: String,
    digest: Option<String>,
) -> Result<(), Error> {
    let digest = digest.map(ConfigDigest::from);
    client()?
        .set_subscription_assignment(&key, &remote, &node, digest)
        .await?;
    println!("Assigned {key} to {remote}/{node}.");
    Ok(())
}

#[api(
    input: {
        properties: {
            key: { schema: SUBSCRIPTION_KEY_SCHEMA },
            digest: {
                schema: PROXMOX_CONFIG_DIGEST_SCHEMA,
                optional: true,
            },
        },
    },
)]
/// Clear the assignment of a key (unassign from its remote node).
async fn clear_assignment(key: String, digest: Option<String>) -> Result<(), Error> {
    let digest = digest.map(ConfigDigest::from);
    client()?
        .clear_subscription_assignment(&key, digest)
        .await?;
    println!("Cleared assignment for {key}.");
    Ok(())
}

#[api(
    input: {
        properties: {
            key: { schema: SUBSCRIPTION_KEY_SCHEMA },
        },
    },
)]
/// Remove a key from the pool entirely.
async fn remove_key(key: String) -> Result<(), Error> {
    client()?.delete_subscription_key(&key).await?;
    println!("Removed {key} from pool.");
    Ok(())
}

#[api(
    input: {
        properties: {
            apply: {
                type: bool,
                optional: true,
                default: false,
                description: "Commit the proposal immediately via bulk-assign. \
                              Without this, only a preview is printed.",
            },
        },
    },
)]
/// Propose (and optionally apply) automatic key-to-node assignments.
async fn auto_assign(apply: bool) -> Result<(), Error> {
    let client = client()?;
    let proposal = client.subscription_auto_assign().await?;

    if proposal.assignments.is_empty() {
        println!("No suitable free keys for nodes without an active subscription.");
        return Ok(());
    }

    let verb = if apply { "assigned" } else { "proposed" };
    for p in &proposal.assignments {
        println!("  {verb}: {} -> {}/{}", p.key, p.remote, p.node);
    }

    if !apply {
        println!("\nRe-run with --apply to apply these assignments.");
        return Ok(());
    }
    let applied = client.subscription_bulk_assign(proposal).await?;
    if applied.is_empty() {
        println!("\nServer rejected the proposal (no entries applied).");
    }
    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: { schema: NODE_SCHEMA },
            digest: {
                schema: PROXMOX_CONFIG_DIGEST_SCHEMA,
                optional: true,
            },
        },
    },
)]
/// Adopt the live subscription on a remote node into the pool.
///
/// Brings a foreign subscription under PDM management without touching the remote: the live
/// current key on `remote`/`node` is imported as a pool entry bound to that node. Refuses if
/// the (remote, node) target already has a pool-managed binding.
async fn adopt_key(remote: String, node: String, digest: Option<String>) -> Result<(), Error> {
    let digest = digest.map(ConfigDigest::from);
    client()?
        .subscription_adopt_key(&remote, &node, digest)
        .await?;
    println!("Adopted live subscription on {remote}/{node} into the pool.");
    Ok(())
}

#[api(
    input: {
        properties: {
            digest: {
                schema: PROXMOX_CONFIG_DIGEST_SCHEMA,
                optional: true,
            },
        },
    },
)]
/// Adopt every foreign live subscription into the pool in one transaction.
///
/// Walks all remotes the caller can audit, imports any (remote, node) with a live current key
/// and no pool binding. Candidates the caller has no modify privilege on, or whose key is
/// already bound elsewhere in the pool, are silently skipped.
async fn adopt_all(digest: Option<String>) -> Result<(), Error> {
    let digest = digest.map(ConfigDigest::from);
    let adopted = client()?.subscription_adopt_all(digest).await?;
    if adopted.is_empty() {
        println!("No foreign live subscriptions to adopt.");
        return Ok(());
    }
    println!("Adopted {} live subscription(s):", adopted.len());
    for e in &adopted {
        println!("  {}/{} -> {}", e.remote, e.node, e.key);
    }
    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: { schema: NODE_SCHEMA },
        },
    },
)]
/// Trigger a fresh shop-side subscription check on a remote node.
///
/// Equivalent to the per-product "Check" button: re-verifies the live subscription status
/// against the shop. Useful for promoting a stale Invalid/Expired verdict to Active once the
/// underlying issue is fixed at the shop, without waiting for the next periodic check.
async fn check_subscription(remote: String, node: String) -> Result<(), Error> {
    client()?.subscription_check(&remote, &node).await?;
    println!("Re-checked subscription on {remote}/{node}.");
    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: { schema: NODE_SCHEMA },
            digest: {
                schema: PROXMOX_CONFIG_DIGEST_SCHEMA,
                optional: true,
            },
        },
    },
)]
/// Drop a queued Clear Key on a remote node while keeping the pool binding.
async fn revert_clear(remote: String, node: String, digest: Option<String>) -> Result<(), Error> {
    let digest = digest.map(ConfigDigest::from);
    client()?
        .subscription_revert_pending_clear(&remote, &node, digest)
        .await?;
    println!("Reverted pending clear on {remote}/{node}.");
    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: { schema: NODE_SCHEMA },
            digest: {
                schema: PROXMOX_CONFIG_DIGEST_SCHEMA,
                optional: true,
            },
        },
    },
)]
/// Queue a Clear Key on a remote node so its subscription can be removed at next Apply Pending.
///
/// Refuses if no pool entry is bound to (remote, node): foreign live subscriptions must first
/// be imported via the explicit Adopt Key action, never as a side effect of queueing a clear.
async fn clear_key(remote: String, node: String, digest: Option<String>) -> Result<(), Error> {
    let digest = digest.map(ConfigDigest::from);
    client()?
        .subscription_queue_clear(&remote, &node, digest)
        .await?;
    println!("Queued Clear Key on {remote}/{node}; run apply-pending to commit.");
    Ok(())
}

#[api(
    input: {
        properties: {
            digest: {
                schema: PROXMOX_CONFIG_DIGEST_SCHEMA,
                optional: true,
            },
        },
    },
)]
/// Push all pending key assignments to remotes as a worker task.
///
/// Blocks until the worker finishes so the operator sees the exit status of the actual push
/// run, not just a UPID they would have to chase down by hand.
async fn apply_pending(digest: Option<String>) -> Result<(), Error> {
    let digest = digest.map(ConfigDigest::from);
    let client = client()?;
    let upid = match client.subscription_apply_pending(digest).await? {
        None => {
            println!("No pending assignments to apply.");
            return Ok(());
        }
        Some(upid) => upid,
    };
    println!("Started worker task: {upid}");
    let status = client.wait_for_local_task(&upid).await?;
    let exit = status
        .get("exitstatus")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    if exit == "OK" {
        println!("Task finished: OK");
        Ok(())
    } else {
        anyhow::bail!("worker task ended with: {exit}");
    }
}

#[api(
    input: {
        properties: {
            digest: {
                schema: PROXMOX_CONFIG_DIGEST_SCHEMA,
                optional: true,
            },
        },
    },
)]
/// Clear every pending assignment in one bulk transaction.
async fn clear_pending(digest: Option<String>) -> Result<(), Error> {
    let digest = digest.map(ConfigDigest::from);
    let cleared = client()?.subscription_clear_pending(digest).await?;
    if cleared == 0 {
        println!("No pending assignments to clear.");
    } else {
        println!("Cleared {cleared} pending assignment(s).");
    }
    Ok(())
}
