//! Subscription key pool management API.
//!
//! Manages a PDM-side pool of subscription keys, proposes key-to-node assignments, and pushes
//! assigned keys to remote nodes. All entries are added manually for now; each entry is a bare
//! `key` string with the product type derived from its prefix.

use std::collections::HashSet;

use anyhow::{bail, format_err, Context, Error};
use futures::future::join_all;

use proxmox_access_control::CachedUserInfo;
use proxmox_config_digest::ConfigDigest;
use proxmox_log::{info, warn};
use proxmox_router::{
    http_bail, http_err, list_subdirs_api_method, Permission, Router, RpcEnvironment, SubdirMap,
};
use proxmox_schema::api;
use proxmox_section_config::typed::SectionConfigData;
use proxmox_sortable_macro::sortable;

use pdm_api_types::remotes::{Remote, REMOTE_ID_SCHEMA};
use pdm_api_types::subscription::{
    pick_best_pve_socket_key, socket_count_from_key, AddKeysResult, AutoAssignProposal,
    ClearPendingResult, ProductType, ProposedAssignment, RemoteNodeStatus, SubscriptionKeyEntry,
    SubscriptionKeySource, SubscriptionLevel, SUBSCRIPTION_KEY_SCHEMA,
};
use pdm_api_types::{
    Authid, NODE_SCHEMA, PRIV_RESOURCE_AUDIT, PRIV_RESOURCE_MODIFY, PRIV_SYS_AUDIT, PRIV_SYS_MODIFY,
};

use crate::api::resources::{
    get_subscription_info_for_remote, invalidate_subscription_info_for_remote,
};

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    (
        "apply-pending",
        &Router::new().post(&API_METHOD_APPLY_PENDING)
    ),
    ("auto-assign", &Router::new().post(&API_METHOD_AUTO_ASSIGN)),
    ("bulk-assign", &Router::new().post(&API_METHOD_BULK_ASSIGN)),
    (
        "clear-pending",
        &Router::new().post(&API_METHOD_CLEAR_PENDING)
    ),
    ("keys", &KEYS_ROUTER),
    ("node-status", &Router::new().get(&API_METHOD_NODE_STATUS)),
]);

const KEYS_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_KEYS)
    .post(&API_METHOD_ADD_KEYS)
    .match_all("key", &KEY_ITEM_ROUTER);

const KEY_ITEM_ROUTER: Router = Router::new()
    .get(&API_METHOD_GET_KEY)
    .delete(&API_METHOD_DELETE_KEY)
    .subdirs(KEY_ITEM_SUBDIRS);

const KEY_ITEM_SUBDIRS: SubdirMap = &[("assignment", &ASSIGNMENT_ROUTER)];

const ASSIGNMENT_ROUTER: Router = Router::new()
    .post(&API_METHOD_SET_ASSIGNMENT)
    .delete(&API_METHOD_CLEAR_ASSIGNMENT);

/// Force-fresh node-status query so the next view reflects the new state instead of returning a
/// cached entry up to 5 minutes later. Used by auto-assign / apply-pending / clear-pending to
/// avoid double-driving a node that has already moved to Active in the cache window.
const FRESH_NODE_STATUS_MAX_AGE: u64 = 0;

/// Cached node-status freshness used by read-only views. Five minutes matches the resource-cache
/// convention and is short enough that admins rarely see stale data on the panel.
const PANEL_NODE_STATUS_MAX_AGE: u64 = 5 * 60;

/// Render a subscription key for worker logs and bail messages without exposing the full secret.
/// Keeps the product prefix and the first/last hex characters of the secret so an operator can
/// still tell two keys apart in a tail of `journalctl`, but the full key never lands in a log
/// file readable by anyone other than the priv user.
fn redact_key(key: &str) -> String {
    let Some((prefix, secret)) = key.split_once('-') else {
        return "<malformed-key>".to_string();
    };
    let mut chars = secret.chars();
    let Some(first) = chars.next() else {
        return format!("{prefix}-...");
    };
    match chars.next_back() {
        Some(last) => format!("{prefix}-{first}...{last}"),
        None => format!("{prefix}-{first}..."),
    }
}

/// Build the canonical 404 error for a missing pool entry.
fn key_not_found(key: &str) -> Error {
    http_err!(NOT_FOUND, "key '{key}' not found in pool")
}

#[api(
    returns: {
        type: Array,
        description: "List of subscription keys in the pool.",
        items: { type: SubscriptionKeyEntry },
    },
    access: {
        permission: &Permission::Privilege(&["system"], PRIV_SYS_AUDIT, false),
    },
)]
/// List all subscription keys in the key pool the caller has audit access to.
///
/// Unbound pool entries are visible to anyone holding the system-AUDIT bit. Bound entries are
/// additionally gated on per-remote `PRIV_RESOURCE_AUDIT` so that an operator who can audit the
/// pool but not a specific remote does not learn which keys are pinned to it (and through that,
/// the existence and rough size of that remote's deployment).
fn list_keys(rpcenv: &mut dyn RpcEnvironment) -> Result<Vec<SubscriptionKeyEntry>, Error> {
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .context("no authid available")?
        .parse()?;
    let user_info = CachedUserInfo::new()?;

    let (config, digest) = pdm_config::subscriptions::config()?;
    rpcenv["digest"] = digest.to_hex().into();
    Ok(config
        .into_iter()
        .filter_map(|(_id, mut entry)| {
            if let Some(remote) = entry.remote.as_deref() {
                if user_info.lookup_privs(&auth_id, &["resource", remote]) & PRIV_RESOURCE_AUDIT
                    == 0
                {
                    return None;
                }
            }
            entry.level = SubscriptionLevel::from_key(Some(&entry.key));
            Some(entry)
        })
        .collect())
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
                type: ConfigDigest,
                optional: true,
            },
        },
    },
    returns: { type: AddKeysResult },
    access: {
        permission: &Permission::Privilege(&["system"], PRIV_SYS_MODIFY, false),
    },
)]
/// Add one or more subscription keys to the pool.
///
/// The key prefix determines the product type via [`ProductType::from_key`]. The schema regex
/// rejects anything that isn't a PVE or PBS key today; widen [`PRODUCT_KEY_REGEX`] in lockstep
/// with `from_key` and `push_key_to_remote` when PMG/POM remote support lands.
///
/// Duplicate keys within the input are silently collapsed to a single entry; the dropped count is
/// reported back so a CLI / API caller can tell pasting the same key twice apart from a genuine
/// add. Keys already present in the pool are still a hard CONFLICT - that is operator-visible
/// state, not an input typo. Apart from dedup the request stays all-or-nothing: a single bad
/// prefix or an existing-pool collision fails the call and leaves the pool untouched.
///
/// The post-save digest is set on the response so clients can chain a follow-up mutation without
/// a refetch round-trip.
async fn add_keys(
    keys: Vec<String>,
    digest: Option<ConfigDigest>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<AddKeysResult, Error> {
    if keys.is_empty() {
        http_bail!(BAD_REQUEST, "no keys provided");
    }

    // Dedup the input first (keep first occurrence); the existing-pool CONFLICT runs later in the
    // locked section, so a key both duplicated and already in the pool yields one CONFLICT.
    let mut entries: Vec<SubscriptionKeyEntry> = Vec::with_capacity(keys.len());
    let mut seen: HashSet<&str> = HashSet::new();
    let mut deduplicated = 0u32;
    for key in &keys {
        if !seen.insert(key.as_str()) {
            deduplicated += 1;
            continue;
        }
        let product_type = ProductType::from_key(key).ok_or_else(|| {
            // Currently unreachable because the schema regex caps inputs to known prefixes, but
            // a future regex widening (PMG/POM) where `from_key` lags behind would fire this -
            // redact defensively so a real key doesn't end up in the journal.
            warn!(
                "rejecting unrecognised key prefix '{}', possibly a new product line",
                redact_key(key),
            );
            http_err!(BAD_REQUEST, "unrecognised key format: {}", redact_key(key))
        })?;
        entries.push(SubscriptionKeyEntry {
            key: key.clone(),
            product_type,
            level: SubscriptionLevel::from_key(Some(key)),
            source: SubscriptionKeySource::Manual,
            ..Default::default()
        });
    }

    let added = entries.len() as u32;
    let new_digest = tokio::task::spawn_blocking(move || -> Result<ConfigDigest, Error> {
        let _lock = pdm_config::subscriptions::lock_config()?;
        let (mut config, config_digest) = pdm_config::subscriptions::config()?;
        config_digest.detect_modification(digest.as_ref())?;

        // `insert` returns the previous entry when one existed; treat that as the duplicate
        // signal. Doing this inline avoids a second pass over `entries` and falls out of the
        // loop on the first collision. The all-or-nothing contract holds because save_config
        // only runs after the loop completes, so a bail on entry N leaves the on-disk pool
        // untouched even if entries 1..N already landed in the in-memory `config`.
        for entry in entries {
            if let Some(existing) = config.insert(entry.key.clone(), entry) {
                http_bail!(CONFLICT, "key '{}' already exists in pool", existing.key);
            }
        }

        pdm_config::subscriptions::save_config(&config)
    })
    .await??;
    rpcenv["digest"] = new_digest.to_hex().into();
    Ok(AddKeysResult {
        added,
        deduplicated,
    })
}

#[api(
    input: {
        properties: {
            key: { schema: SUBSCRIPTION_KEY_SCHEMA },
        },
    },
    returns: { type: SubscriptionKeyEntry },
    access: {
        permission: &Permission::Privilege(&["system"], PRIV_SYS_AUDIT, false),
    },
)]
/// Get details for a single key.
///
/// Bound entries are hidden from operators who cannot audit the bound remote (mirrors the
/// `list_keys` filter); the response is the same 404 either way so a probe cannot distinguish
/// "key exists but you cannot see it" from "key not in pool".
fn get_key(key: String, rpcenv: &mut dyn RpcEnvironment) -> Result<SubscriptionKeyEntry, Error> {
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .context("no authid available")?
        .parse()?;
    let user_info = CachedUserInfo::new()?;

    let (config, digest) = pdm_config::subscriptions::config()?;
    rpcenv["digest"] = digest.to_hex().into();
    let mut entry = config
        .get(&key)
        .cloned()
        .ok_or_else(|| key_not_found(&key))?;

    if let Some(remote) = entry.remote.as_deref() {
        if user_info.lookup_privs(&auth_id, &["resource", remote]) & PRIV_RESOURCE_AUDIT == 0 {
            return Err(key_not_found(&key));
        }
    }

    entry.level = SubscriptionLevel::from_key(Some(&entry.key));
    Ok(entry)
}

#[api(
    // Required because save_shadow writes a priv:priv 0600 file (signed-blob storage); only the
    // privileged daemon can chown to that uid.
    protected: true,
    input: {
        properties: {
            key: { schema: SUBSCRIPTION_KEY_SCHEMA },
            digest: {
                type: ConfigDigest,
                optional: true,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system"], PRIV_SYS_MODIFY, false),
    },
)]
/// Remove a key from the pool.
///
/// If the key is currently assigned to a remote node, the caller must also have
/// `PRIV_RESOURCE_MODIFY` on that remote, so an audit-only operator cannot release a key
/// another admin had pinned. Refuses if the key is currently the live active key on its bound
/// node, since dropping the pool entry would orphan that subscription on the remote: the
/// operator must release the live subscription on the remote first.
async fn delete_key(
    key: String,
    digest: Option<ConfigDigest>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<(), Error> {
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .context("no authid available")?
        .parse()?;
    let user_info = CachedUserInfo::new()?;

    // Authorise the caller against the entry's bound remote BEFORE hitting the network: an
    // operator with only PRIV_SYS_MODIFY should not be able to probe live subscription state on
    // a remote they cannot audit. Read the entry once without the lock for this gate; the
    // authoritative read happens under the spawn_blocking section below.
    let (pre_config, pre_digest) = pdm_config::subscriptions::config()?;
    let Some(pre_entry) = pre_config.get(&key) else {
        return Err(key_not_found(&key));
    };
    if let Some(assigned_remote) = pre_entry.remote.as_deref() {
        user_info.check_privs(
            &auth_id,
            &["resource", assigned_remote],
            PRIV_RESOURCE_MODIFY,
            false,
        )?;
    }

    // Live fetch must happen before the lock since the lock cannot span an .await. Pass the
    // pre-read binding so the helper hits only a remote we already priv-checked above: a
    // parallel rebind to a remote we cannot AUDIT would otherwise probe that remote here.
    let pre_binding = pre_entry.remote.as_deref().zip(pre_entry.node.as_deref());
    // Owned bool so the orphan guard inside spawn_blocking does not borrow `pre_config`.
    let pre_had_binding = pre_binding.is_some();
    let synced_block = check_synced_assignment_for_unassign(&key, pre_binding).await?;
    drop(pre_config);

    // The lock + sync IO runs on a blocking thread so the async runtime is free for other work
    // even when /etc/proxmox-datacenter-manager/subscriptions is on slow storage. The
    // post-lock priv re-check is duplicated inside the closure since `user_info` cannot easily
    // cross the boundary; reconstructing it is cheap (it just reads the shared ACL cache).
    let new_digest = tokio::task::spawn_blocking(move || -> Result<ConfigDigest, Error> {
        let user_info = CachedUserInfo::new()?;
        let _lock = pdm_config::subscriptions::lock_config()?;
        let (mut config, config_digest) = pdm_config::subscriptions::config()?;
        config_digest.detect_modification(digest.as_ref())?;
        let mut shadow = pdm_config::subscriptions::shadow_config()?;

        let Some(entry) = config.get(&key) else {
            return Err(key_not_found(&key));
        };

        // Re-check the priv on the locked entry: a parallel rebind during the live fetch could
        // have moved the binding to a remote the caller cannot modify.
        if let Some(assigned_remote) = entry.remote.as_deref() {
            user_info.check_privs(
                &auth_id,
                &["resource", assigned_remote],
                PRIV_RESOURCE_MODIFY,
                false,
            )?;
        }

        // Orphan guard: refuse on any post-lock divergence that makes the pre-lock live check
        // stale (still synced; digest moved while bound; binding appeared during the .await).
        let bound_post = entry.remote.is_some();
        let raced = config_digest != pre_digest;
        let still_synced = synced_block
            .as_ref()
            .map(|(r, n)| {
                entry.remote.as_deref() == Some(r.as_str())
                    && entry.node.as_deref() == Some(n.as_str())
            })
            .unwrap_or(false);
        let appeared_unchecked = !pre_had_binding && bound_post;
        if still_synced || (raced && bound_post) || appeared_unchecked {
            http_bail!(
                BAD_REQUEST,
                "key '{key}' is currently bound to a remote node with a live active \
                 subscription; release it on the remote first"
            );
        }

        config.remove(&key);
        shadow.remove(&key);
        // Save main config first: an interrupted remove must not leave a `key` entry whose
        // signed blob is gone (other readers would see the entry and try to consult the
        // missing shadow). A stale shadow blob with no main entry is benign - readers do not
        // consult it.
        let new_digest = pdm_config::subscriptions::save_config(&config)?;
        pdm_config::subscriptions::save_shadow(&shadow)?;
        Ok(new_digest)
    })
    .await??;
    rpcenv["digest"] = new_digest.to_hex().into();

    Ok(())
}

#[api(
    input: {
        properties: {
            key: { schema: SUBSCRIPTION_KEY_SCHEMA },
            remote: { schema: REMOTE_ID_SCHEMA },
            // NODE_SCHEMA rejects path-traversal input before it ends up interpolated into the
            // remote URL `/api2/extjs/nodes/{node}/subscription`.
            node: { schema: NODE_SCHEMA },
            digest: {
                type: ConfigDigest,
                optional: true,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system"], PRIV_SYS_MODIFY, false),
    },
)]
/// Bind a pool key to a remote node.
///
/// `PRIV_SYS_MODIFY` lets the caller touch the pool config; per-remote `PRIV_RESOURCE_MODIFY`
/// is enforced inside this handler so an operator cannot push a key to a remote they have no
/// other authority on.
async fn set_assignment(
    key: String,
    remote: String,
    node: String,
    digest: Option<ConfigDigest>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<(), Error> {
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .context("no authid available")?
        .parse()?;
    let user_info = CachedUserInfo::new()?;
    user_info.check_privs(
        &auth_id,
        &["resource", &remote],
        PRIV_RESOURCE_MODIFY,
        false,
    )?;

    // Pre-lock orphan-prevention for the rebind path: pushing the same key to a NEW remote at
    // the next Apply Pending makes the shop reissue the serverid against the new remote and
    // orphans whatever live subscription the old remote still ran. Same shape and same guard
    // as delete_key / clear_assignment; only fires when the binding actually moves (re-set to
    // the same target leaves the OLD binding intact and carries no orphan risk).
    let (pre_config, pre_digest) = pdm_config::subscriptions::config()?;
    let pre_entry = pre_config.get(&key);
    let pre_binding = pre_entry.and_then(|e| e.remote.as_deref().zip(e.node.as_deref()));
    let rebind_moves_binding = match pre_binding {
        Some((r, n)) => r != remote.as_str() || n != node.as_str(),
        None => false,
    };
    if rebind_moves_binding {
        if let Some((prev_remote, _)) = pre_binding {
            // Reassigning away from a previous remote requires modify on that remote too,
            // otherwise an audit-only-on-A operator could effectively pull a key off A by
            // re-binding it to a remote B they can modify and applying the push (which makes
            // the shop reissue the serverid to B and invalidates A).
            user_info.check_privs(
                &auth_id,
                &["resource", prev_remote],
                PRIV_RESOURCE_MODIFY,
                false,
            )?;
        }
    }
    let pre_had_binding = pre_binding.is_some();
    let synced_block = if rebind_moves_binding {
        check_synced_assignment_for_unassign(&key, pre_binding).await?
    } else {
        None
    };
    drop(pre_config);

    // Lock + sync IO under spawn_blocking so the async runtime stays free during the file
    // operations. `user_info` is reconstructed inside the closure since the priv check happens
    // under the lock.
    let new_digest = tokio::task::spawn_blocking(move || -> Result<ConfigDigest, Error> {
        let user_info = CachedUserInfo::new()?;
        let _lock = pdm_config::subscriptions::lock_config()?;
        let (mut config, config_digest) = pdm_config::subscriptions::config()?;
        config_digest.detect_modification(digest.as_ref())?;

        let Some(stored_entry) = config.get(&key).cloned() else {
            return Err(key_not_found(&key));
        };
        let product_type = stored_entry.product_type;

        // Re-check the priv on the locked entry: a parallel rebind during the live fetch could
        // have moved the binding to a remote the caller cannot modify.
        if let Some(prev_remote) = stored_entry.remote.as_deref() {
            if prev_remote != remote {
                user_info.check_privs(
                    &auth_id,
                    &["resource", prev_remote],
                    PRIV_RESOURCE_MODIFY,
                    false,
                )?;
            }
        }

        // Orphan guard for the rebind path: refuse on any post-lock divergence that makes the
        // pre-lock live check stale (still synced; digest moved while bound; binding appeared
        // during the .await). Only fires when the binding moves: re-set to the same target
        // leaves the old binding intact and is not a rebind.
        let bound_post = stored_entry.remote.is_some();
        let post_moves = match (stored_entry.remote.as_deref(), stored_entry.node.as_deref()) {
            (Some(r), Some(n)) => r != remote.as_str() || n != node.as_str(),
            _ => false,
        };
        let raced = config_digest != pre_digest;
        let still_synced = synced_block
            .as_ref()
            .map(|(r, n)| {
                stored_entry.remote.as_deref() == Some(r.as_str())
                    && stored_entry.node.as_deref() == Some(n.as_str())
            })
            .unwrap_or(false);
        let appeared_unchecked = !pre_had_binding && bound_post && post_moves;
        if (still_synced && post_moves) || (raced && bound_post && post_moves) || appeared_unchecked
        {
            http_bail!(
                BAD_REQUEST,
                "key '{key}' is currently bound to a remote node with a live active \
                 subscription; release it on the remote before rebinding"
            );
        }

        let (remotes_config, _) = pdm_config::remotes::config()?;
        let remote_entry = remotes_config
            .get(&remote)
            .ok_or_else(|| http_err!(NOT_FOUND, "remote '{remote}' not found"))?;

        if !product_type.matches_remote_type(remote_entry.ty) {
            http_bail!(
                BAD_REQUEST,
                "key type '{product_type}' does not match remote type '{}'",
                remote_entry.ty
            );
        }

        for (_id, other) in config.iter() {
            if other.key != key
                && other.remote.as_deref() == Some(remote.as_str())
                && other.node.as_deref() == Some(node.as_str())
            {
                http_bail!(
                    CONFLICT,
                    "key '{}' is already assigned to {remote}/{node}",
                    other.key
                );
            }
        }

        // Safe: the earlier `config.get(&key).cloned()` above proved the key exists, and the
        // `_lock` guard keeps the config stable across this section.
        let entry = config
            .get_mut(&key)
            .expect("entry verified to exist under lock above");
        entry.remote = Some(remote);
        entry.node = Some(node);

        pdm_config::subscriptions::save_config(&config)
    })
    .await??;
    rpcenv["digest"] = new_digest.to_hex().into();

    Ok(())
}

#[api(
    input: {
        properties: {
            key: { schema: SUBSCRIPTION_KEY_SCHEMA },
            digest: {
                type: ConfigDigest,
                optional: true,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system"], PRIV_SYS_MODIFY, false),
    },
)]
/// Drop the remote-node binding for a pool key.
///
/// Refuses when the binding is currently synced (the assigned key is the live active key on
/// its remote): unassigning then would orphan that subscription, so the operator must release
/// the live subscription on the remote first.
async fn clear_assignment(
    key: String,
    digest: Option<ConfigDigest>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<(), Error> {
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .context("no authid available")?
        .parse()?;
    let user_info = CachedUserInfo::new()?;

    // Authorise against the entry's bound remote BEFORE hitting the network. An operator with
    // only PRIV_SYS_MODIFY should not be able to probe live subscription state on a remote
    // they cannot audit. The authoritative re-check happens after the lock below.
    let (pre_config, pre_digest) = pdm_config::subscriptions::config()?;
    let pre_entry = pre_config.get(&key);
    if let Some(pre_entry) = pre_entry {
        if let Some(assigned_remote) = pre_entry.remote.as_deref() {
            user_info.check_privs(
                &auth_id,
                &["resource", assigned_remote],
                PRIV_RESOURCE_MODIFY,
                false,
            )?;
        }
    }

    // The live fetch must happen before the lock since the lock cannot span an .await. Snapshot
    // the digest above so the post-lock check below can refuse if a parallel admin rebound the
    // entry during the .await - in that race the original remote still has our live
    // subscription and unbinding here would orphan it. Pass the pre-read binding so the helper
    // hits only the remote the pre-priv check above already covered.
    let pre_binding = pre_entry.and_then(|e| e.remote.as_deref().zip(e.node.as_deref()));
    // Owned bool so the orphan guard inside spawn_blocking does not borrow `pre_config`.
    let pre_had_binding = pre_binding.is_some();
    let synced_block = check_synced_assignment_for_unassign(&key, pre_binding).await?;
    drop(pre_config);

    // The lock + sync IO runs on a blocking thread so the async runtime is free for other work
    // even when /etc/proxmox-datacenter-manager/subscriptions is on slow storage. The post-lock
    // priv re-check is duplicated inside the closure since `user_info` cannot easily cross the
    // boundary; reconstructing it is cheap (it just reads the shared ACL cache).
    let new_digest = tokio::task::spawn_blocking(move || -> Result<ConfigDigest, Error> {
        let user_info = CachedUserInfo::new()?;
        let _lock = pdm_config::subscriptions::lock_config()?;
        let (mut config, config_digest) = pdm_config::subscriptions::config()?;
        config_digest.detect_modification(digest.as_ref())?;

        let Some(stored_entry) = config.get(&key).cloned() else {
            return Err(key_not_found(&key));
        };

        // Re-check the priv on the locked entry: a parallel rebind during the live fetch could
        // have moved the binding to a remote the caller cannot modify.
        if let Some(prev_remote) = stored_entry.remote.as_deref() {
            user_info.check_privs(
                &auth_id,
                &["resource", prev_remote],
                PRIV_RESOURCE_MODIFY,
                false,
            )?;
        }

        // Orphan guard: refuse on any post-lock divergence that makes the pre-lock live check
        // stale (still synced; digest moved while bound; binding appeared during the .await).
        let bound_post = stored_entry.remote.is_some();
        let raced = config_digest != pre_digest;
        let still_synced = synced_block
            .as_ref()
            .map(|(r, n)| {
                stored_entry.remote.as_deref() == Some(r.as_str())
                    && stored_entry.node.as_deref() == Some(n.as_str())
            })
            .unwrap_or(false);
        let appeared_unchecked = !pre_had_binding && bound_post;
        if still_synced || (raced && bound_post) || appeared_unchecked {
            http_bail!(
                BAD_REQUEST,
                "key '{key}' is currently bound to a remote node with a live active \
                 subscription; release it on the remote first"
            );
        }
        // Safe: the earlier `config.get(&key).cloned()` above proved the key exists, and the
        // `_lock` guard keeps the config stable across this section.
        let entry = config
            .get_mut(&key)
            .expect("entry verified to exist under lock above");
        entry.remote = None;
        entry.node = None;

        pdm_config::subscriptions::save_config(&config)
    })
    .await??;
    rpcenv["digest"] = new_digest.to_hex().into();

    Ok(())
}

/// Pre-lock check for the unassign / delete-key paths ([`clear_assignment`] and [`delete_key`]):
/// returns the (remote, node) the entry is currently active on, if any, so the lock-protected
/// branch can refuse the operation and prompt the operator to release the live subscription
/// on the remote first. Returns `None` for entries with no binding, no live subscription, or
/// a live subscription whose key does not match the entry.
///
/// Takes the binding from the caller's pre-read entry rather than re-reading config so the
/// remote we hit on the network is the one the caller's pre-priv check already covered: a
/// parallel rebind between pre-read and here cannot redirect us at a remote the caller has no
/// AUDIT on.
async fn check_synced_assignment_for_unassign(
    key: &str,
    binding: Option<(&str, &str)>,
) -> Result<Option<(String, String)>, Error> {
    let Some((prev_remote, prev_node)) = binding else {
        return Ok(None);
    };
    let (remotes_config, _) = pdm_config::remotes::config()?;
    let Some(remote_entry) = remotes_config.get(prev_remote) else {
        return Ok(None);
    };
    let live = match get_subscription_info_for_remote(remote_entry, FRESH_NODE_STATUS_MAX_AGE).await
    {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let synced = live
        .get(prev_node)
        .and_then(|info| info.as_ref())
        .map(|info| {
            info.status == proxmox_subscription::SubscriptionStatus::Active
                && info.key.as_deref() == Some(key)
        })
        .unwrap_or(false);
    Ok(synced.then_some((prev_remote.to_string(), prev_node.to_string())))
}

/// Push a single key to its assigned remote node. Operates on a borrowed `Remote` so the
/// caller can fetch the remotes-config once and reuse it.
async fn push_key_to_remote(remote: &Remote, key: &str, node_name: &str) -> Result<(), Error> {
    let product_type =
        ProductType::from_key(key).ok_or_else(|| format_err!("unrecognised key format: {key}"))?;

    match product_type {
        ProductType::Pve => {
            let client = crate::connection::make_pve_client(remote)?;
            client
                .set_subscription(
                    node_name,
                    pve_api_types::SetSubscription {
                        key: key.to_string(),
                    },
                )
                .await?;
        }
        ProductType::Pbs => {
            let client = crate::connection::make_pbs_client(remote)?;
            client
                .set_subscription(proxmox_subscription::SetSubscription {
                    key: key.to_string(),
                })
                .await?;
        }
        ProductType::Pmg | ProductType::Pom => {
            bail!("PDM cannot push '{product_type}' keys: no remote support yet");
        }
    }

    info!(
        "pushed key '{}' to {}/{node_name}",
        redact_key(key),
        remote.id,
    );
    Ok(())
}

#[api(
    input: {
        properties: {
            "max-age": {
                type: u64,
                optional: true,
                description: "Override the cache freshness window in seconds. \
                              Default 300 for panel views; pass 0 to force a fresh query.",
            },
        },
    },
    returns: {
        type: Array,
        description: "Subscription status of all remote nodes the user can audit.",
        items: { type: RemoteNodeStatus },
    },
    access: {
        permission: &Permission::Privilege(&["system"], PRIV_SYS_AUDIT, false),
    },
)]
/// Get the subscription status of every remote node the caller can audit, combined with key pool
/// assignment information.
///
/// Per-remote `PRIV_RESOURCE_AUDIT` is enforced inside the handler so users only see remotes
/// they may audit.
async fn node_status(
    max_age: Option<u64>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<RemoteNodeStatus>, Error> {
    collect_node_status(max_age.unwrap_or(PANEL_NODE_STATUS_MAX_AGE), rpcenv).await
}

/// Shared helper: fan out subscription queries to all remotes the caller has audit privilege on,
/// in parallel, reusing the per-remote API cache via `get_subscription_info_for_remote`.
/// Joins the results with the key-pool assignment table.
async fn collect_node_status(
    max_age: u64,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<RemoteNodeStatus>, Error> {
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .context("no authid available")?
        .parse()?;
    let user_info = CachedUserInfo::new()?;

    let visible_remotes: Vec<(String, Remote)> = crate::api::remotes::RemoteIterator::new()?
        .any_privs(&user_info, &auth_id, PRIV_RESOURCE_AUDIT)
        .into_iter()
        .collect();

    let (keys_config, _) = pdm_config::subscriptions::config()?;

    // `get_subscription_info_for_remote` re-uses the per-remote API cache so this
    // fan-out is safe to run concurrently.
    let fetch = visible_remotes.iter().map(|(name, remote)| async move {
        let res = get_subscription_info_for_remote(remote, max_age).await;
        (name.clone(), remote.ty, res)
    });
    let results = join_all(fetch).await;

    let mut out = Vec::new();
    for (remote_name, remote_ty, result) in results {
        let node_infos = match result {
            Ok(info) => info,
            Err(err) => {
                warn!("failed to query subscription for remote {remote_name}: {err}");
                continue;
            }
        };

        for (node_name, node_info) in &node_infos {
            let (status, level, sockets, current_key) = match node_info {
                Some(info) => (info.status, info.level, info.sockets, info.key.clone()),
                None => (
                    proxmox_subscription::SubscriptionStatus::NotFound,
                    SubscriptionLevel::None,
                    None,
                    None,
                ),
            };

            let pool_entry = keys_config.iter().find(|(_id, entry)| {
                entry.remote.as_deref() == Some(remote_name.as_str())
                    && entry.node.as_deref() == Some(node_name.as_str())
            });
            let (assigned_key, pending_clear) = match pool_entry {
                Some((_id, entry)) => (Some(entry.key.clone()), entry.pending_clear),
                None => (None, false),
            };

            out.push(RemoteNodeStatus {
                remote: remote_name.clone(),
                ty: remote_ty,
                node: node_name.to_string(),
                sockets,
                status,
                level,
                assigned_key,
                current_key,
                pending_clear,
            });
        }
    }

    out.sort_by(|a, b| (&a.remote, &a.node).cmp(&(&b.remote, &b.node)));
    Ok(out)
}

#[api(
    returns: { type: AutoAssignProposal },
    access: {
        permission: &Permission::Privilege(&["system"], PRIV_SYS_MODIFY, false),
    },
)]
/// Compute a proposed mapping of unused pool keys to nodes without an active subscription.
///
/// Returns the plan plus snapshots of the inputs (pool digest and a hash of the consulted
/// node-status). The plan is committed by `bulk_assign` and rejected there if either snapshot no
/// longer matches the live state, so an operator never silently applies a plan that drifted
/// between preview and commit.
///
/// `PRIV_SYS_MODIFY` is required to *preview* the plan; the actual commit performed by
/// `bulk_assign` additionally drops proposals on any remote the caller cannot
/// `PRIV_RESOURCE_MODIFY`, so an audit-only-on-a-remote operator can see the suggestion but the
/// write never lands there.
///
/// The response carries nested `AutoAssignProposal` data; clients must submit follow-up
/// `bulk_assign` calls with an `application/json` body, the form-urlencoded path cannot encode
/// the nested structure.
async fn auto_assign(rpcenv: &mut dyn RpcEnvironment) -> Result<AutoAssignProposal, Error> {
    let node_statuses = collect_node_status(FRESH_NODE_STATUS_MAX_AGE, rpcenv).await?;
    let (config, keys_digest) = pdm_config::subscriptions::config()?;
    let assignments = compute_proposals(&config, &node_statuses);
    Ok(AutoAssignProposal {
        assignments,
        keys_digest,
        node_status_digest: hash_node_status(&node_statuses),
    })
}

#[api(
    input: {
        properties: {
            proposal: { type: AutoAssignProposal },
        },
    },
    returns: {
        type: Array,
        description: "Assignments that were actually persisted.",
        items: { type: ProposedAssignment },
    },
    access: {
        permission: &Permission::Privilege(&["system"], PRIV_SYS_MODIFY, false),
    },
)]
/// Apply a proposal previously returned by `auto_assign`.
///
/// Rejects with 409 if the pool config digest has moved or the live node-status hash differs
/// from what the proposal was computed against; the caller is expected to refresh the proposal
/// and retry. Per-remote `PRIV_RESOURCE_MODIFY` is checked inside the handler so an audit-only
/// caller's previously-rendered preview cannot be applied on their behalf.
///
/// The request body is a nested `AutoAssignProposal`; callers must submit it as
/// `application/json`, the form-urlencoded path cannot encode the nested structure.
async fn bulk_assign(
    proposal: AutoAssignProposal,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<ProposedAssignment>, Error> {
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .context("no authid available")?
        .parse()?;

    let node_statuses = collect_node_status(FRESH_NODE_STATUS_MAX_AGE, rpcenv).await?;
    let live_digest = hash_node_status(&node_statuses);
    if live_digest != proposal.node_status_digest {
        http_bail!(
            CONFLICT,
            "node status changed since proposal; refresh and try again"
        );
    }

    // Lock + sync IO under spawn_blocking so the async runtime stays free during the file
    // operations. `user_info` and `auth_id` are reconstructed/cloned into the closure since the
    // priv lookups for every proposal entry happen under the lock.
    let (applied, new_digest_opt) = tokio::task::spawn_blocking(
        move || -> Result<(Vec<ProposedAssignment>, Option<ConfigDigest>), Error> {
            let user_info = CachedUserInfo::new()?;
            let _lock = pdm_config::subscriptions::lock_config()?;
            let (mut config, config_digest) = pdm_config::subscriptions::config()?;
            config_digest.detect_modification(Some(&proposal.keys_digest))?;
            let (remotes_config, _) = pdm_config::remotes::config()?;

            let mut applied = Vec::with_capacity(proposal.assignments.len());
            for p in &proposal.assignments {
                // Audit-only callers may see a remote in the preview but must not be able to
                // stage a write for it that another admin would later push on their behalf.
                if user_info.lookup_privs(&auth_id, &["resource", &p.remote]) & PRIV_RESOURCE_MODIFY
                    == 0
                {
                    continue;
                }
                // The proposal is client-controlled (a malicious client could submit a
                // fabricated `p.node`) and was originally sourced from each remote's
                // node-status reply (a compromised or buggy remote could inject a
                // path-traversal token). Re-validate against NODE_SCHEMA before persisting; the
                // node string later interpolates into the remote URL in `push_key_to_remote`,
                // so this is the only line of defence at that boundary.
                if NODE_SCHEMA.parse_simple_value(&p.node).is_err() {
                    warn!(
                        "skipping bulk-assign entry with invalid node name from proposal: \
                         remote={} (raw node rejected)",
                        p.remote,
                    );
                    continue;
                }
                // Mirror set_assignment's invariants: a client-fabricated proposal must not be
                // able to bind a PVE key to a PBS remote (apply-pending would fail leaving the
                // pool inconsistent), nor double-bind a single (remote, node) target.
                let Some(remote_entry) = remotes_config.get(&p.remote) else {
                    continue;
                };
                let Some(pool_entry) = config.get(&p.key) else {
                    continue;
                };
                if !pool_entry.product_type.matches_remote_type(remote_entry.ty) {
                    continue;
                }
                if config.iter().any(|(_, e)| {
                    e.key != p.key
                        && e.remote.as_deref() == Some(p.remote.as_str())
                        && e.node.as_deref() == Some(p.node.as_str())
                }) {
                    continue;
                }
                if let Some(entry) = config.get_mut(&p.key) {
                    // Defensive: with the digest check above the entry should still be unbound,
                    // but a bug in the proposal computation could otherwise overwrite a foreign
                    // binding.
                    if entry.remote.is_none() {
                        entry.remote = Some(p.remote.clone());
                        entry.node = Some(p.node.clone());
                        applied.push(p.clone());
                    }
                }
            }

            let new_digest = if applied.is_empty() {
                None
            } else {
                Some(pdm_config::subscriptions::save_config(&config)?)
            };
            Ok((applied, new_digest))
        },
    )
    .await??;

    if let Some(new_digest) = new_digest_opt {
        rpcenv["digest"] = new_digest.to_hex().into();
    }

    Ok(applied)
}

/// Stable hash of the slice of node-status fields consulted by `compute_proposals`. Changing
/// what `compute_proposals` reads requires updating this digest to match, otherwise the
/// preview/commit guarantee breaks silently.
fn hash_node_status(statuses: &[RemoteNodeStatus]) -> String {
    let mut keyed: Vec<(
        &str,
        &str,
        proxmox_subscription::SubscriptionStatus,
        Option<i64>,
        bool,
    )> = statuses
        .iter()
        .map(|n| {
            (
                n.remote.as_str(),
                n.node.as_str(),
                n.status,
                n.sockets,
                n.assigned_key.is_some(),
            )
        })
        .collect();
    keyed.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    let raw = serde_json::to_vec(&keyed).unwrap_or_default();
    hex::encode(openssl::sha::sha256(&raw))
}

fn compute_proposals(
    config: &SectionConfigData<SubscriptionKeyEntry>,
    node_statuses: &[RemoteNodeStatus],
) -> Vec<ProposedAssignment> {
    let mut target_nodes: Vec<&RemoteNodeStatus> = node_statuses
        .iter()
        .filter(|n| {
            n.assigned_key.is_none() && n.status != proxmox_subscription::SubscriptionStatus::Active
        })
        .collect();

    // Sort PVE nodes by socket count descending so large nodes get keys first.
    target_nodes.sort_by_key(|n| std::cmp::Reverse(n.sockets.unwrap_or(0)));

    let mut proposals: Vec<ProposedAssignment> = Vec::new();
    let mut taken: HashSet<String> = HashSet::new();

    for node in &target_nodes {
        let remote_type = node.ty;

        let candidates = config.iter().filter(|(id, entry)| {
            entry.remote.is_none()
                && !taken.contains(*id)
                && entry.product_type.matches_remote_type(remote_type)
        });

        let best_key = if remote_type == pdm_api_types::remotes::RemoteType::Pve {
            let node_sockets = node.sockets.unwrap_or(1) as u32;
            pick_best_pve_socket_key(
                node_sockets,
                candidates.map(|(id, entry)| (id.to_string(), entry.key.as_str())),
            )
        } else {
            candidates.map(|(id, _)| id.to_string()).next()
        };

        if let Some(key_id) = best_key {
            let ks = config
                .get(&key_id)
                .and_then(|e| socket_count_from_key(&e.key));
            taken.insert(key_id.clone());
            proposals.push(ProposedAssignment {
                key: key_id,
                remote: node.remote.clone(),
                node: node.node.clone(),
                key_sockets: ks,
                node_sockets: node.sockets,
            });
        }
    }

    proposals
}

#[api(
    input: {
        properties: {
            digest: {
                type: ConfigDigest,
                optional: true,
            },
        },
    },
    returns: {
        schema: pdm_api_types::UPID_SCHEMA,
        optional: true,
    },
    access: {
        permission: &Permission::Privilege(&["system"], PRIV_SYS_MODIFY, false),
    },
)]
/// Apply every pending pool change to its remote node.
///
/// Pending entries are pool keys whose live `current_key` on the bound node does not match the
/// assigned pool key (either a different live key, no key, or the remote did not respond / the
/// node is gone). Each step is logged from a worker task so the admin can follow progress.
///
/// Subscription health (Invalid, Expired, ...) is intentionally not considered pending: the
/// assigned key already reached the node, re-pushing it would not change the shop's verdict.
///
/// The worker bails on the first failure; the remaining entries stay pending so the operator
/// can fix the underlying issue (or clear that one assignment) and trigger another apply.
///
/// Returns `None` when nothing is pending so the caller can show a short info message instead of
/// opening a task progress dialog for a no-op worker.
///
/// The optional `digest` rejects the call at the API boundary if the pool changed since the
/// caller last loaded it, so a stale browser tab cannot start a worker on a plan the operator
/// no longer sees. The worker itself deliberately re-reads the pool when it fires (a worker can
/// be scheduled with delay), so a parallel admin edit between API return and worker firing is
/// still honoured - the digest only pins the at-API-call-time plan, not the executed plan.
async fn apply_pending(
    digest: Option<ConfigDigest>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Option<String>, Error> {
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .context("no authid available")?
        .parse()?;
    let user_info = CachedUserInfo::new()?;

    let (_, config_digest) = pdm_config::subscriptions::config()?;
    config_digest.detect_modification(digest.as_ref())?;

    let node_statuses = collect_node_status(FRESH_NODE_STATUS_MAX_AGE, rpcenv).await?;
    let pending = compute_pending(&user_info, &auth_id, &node_statuses)?;

    if pending.is_empty() {
        return Ok(None);
    }

    let worker_auth = auth_id.clone();
    let upid = proxmox_rest_server::WorkerTask::spawn(
        "subscription-apply-pending",
        None,
        auth_id.to_string(),
        true,
        move |_worker| async move { run_apply_pending(worker_auth).await },
    )?;

    Ok(Some(upid))
}

/// Re-validate and run the apply-pending plan from inside a worker.
///
/// The worker re-reads remotes and the pool config so a reassign or removal between the API call
/// returning a UPID and the worker firing is honoured (pushing the old key to a node after the
/// operator retracted the assignment was a real footgun).
async fn run_apply_pending(auth_id: Authid) -> Result<(), Error> {
    let user_info = CachedUserInfo::new()?;
    let (remotes_config, _) = pdm_config::remotes::config()?;
    let (config, _) = pdm_config::subscriptions::config()?;

    let node_statuses = collect_status_uncached(&remotes_config).await;
    let pending = compute_pending(&user_info, &auth_id, &node_statuses)?;

    if pending.is_empty() {
        info!("apply-pending: nothing to do (state changed since the API call)");
        return Ok(());
    }

    let total = pending.len();
    let mut ok = 0usize;

    for entry in pending {
        let Some(remote) = remotes_config.get(&entry.remote) else {
            bail!(
                "remote '{}' vanished, aborting after {ok}/{total} successful pushes",
                entry.remote,
            );
        };
        // Honour the case where the operator unassigned the key while the worker was queued.
        if !pool_assignment_still_valid(&config, &entry) {
            info!(
                "skipping {}/{}: pool assignment changed before worker ran",
                entry.remote, entry.node
            );
            continue;
        }

        let redacted = redact_key(&entry.key);
        info!("pushing {redacted} to {}/{}...", entry.remote, entry.node);
        if let Err(err) = push_key_to_remote(remote, &entry.key, &entry.node).await {
            bail!(
                "push of {redacted} to {}/{} failed after {ok}/{total} successful pushes: {err}",
                entry.remote,
                entry.node,
            );
        }
        info!("  success");
        invalidate_subscription_info_for_remote(&entry.remote).await;
        ok += 1;
    }

    info!("finished: {ok}/{total} pushes succeeded");
    Ok(())
}

#[api(
    input: {
        properties: {
            digest: {
                type: ConfigDigest,
                optional: true,
            },
        },
    },
    returns: { type: ClearPendingResult },
    access: {
        permission: &Permission::Privilege(&["system"], PRIV_SYS_MODIFY, false),
    },
)]
/// Clear every pending assignment in one bulk transaction.
///
/// Pending = pool key bound to a remote node whose live `current_key` does not match the
/// assigned pool key (a different live key, no key, or no row returned at all because the remote
/// is unreachable / the node is gone). Clears only those entries the caller has
/// `PRIV_RESOURCE_MODIFY` on; remotes the caller may only audit are skipped. Mirrors
/// `apply-pending` but drops the assignments instead of pushing them, so an operator can disown
/// stuck assignments without first having to bring the target back online.
///
/// The optional `digest` is checked twice: once before the live-state fetch so a stale browser
/// tab is rejected up-front, and again under the config lock so a parallel admin edit between
/// fetch and write does not get silently overwritten.
async fn clear_pending(
    digest: Option<ConfigDigest>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<ClearPendingResult, Error> {
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .context("no authid available")?
        .parse()?;
    let user_info = CachedUserInfo::new()?;

    let (_, pre_digest) = pdm_config::subscriptions::config()?;
    pre_digest.detect_modification(digest.as_ref())?;

    let node_statuses = collect_node_status(FRESH_NODE_STATUS_MAX_AGE, rpcenv).await?;
    let pending = compute_pending(&user_info, &auth_id, &node_statuses)?;

    if pending.is_empty() {
        return Ok(ClearPendingResult { cleared: 0 });
    }

    // Lock + sync IO under spawn_blocking so the async runtime stays free during the file
    // operations.
    let (cleared, new_digest_opt) =
        tokio::task::spawn_blocking(move || -> Result<(u32, Option<ConfigDigest>), Error> {
            let _lock = pdm_config::subscriptions::lock_config()?;
            let (mut config, locked_digest) = pdm_config::subscriptions::config()?;
            locked_digest.detect_modification(digest.as_ref())?;

            let mut cleared: u32 = 0;
            for entry in &pending {
                // Re-check inside the lock so a concurrent reassign is not silently
                // overwritten.
                if let Some(stored) = config.get_mut(&entry.key) {
                    if stored.remote.as_deref() == Some(entry.remote.as_str())
                        && stored.node.as_deref() == Some(entry.node.as_str())
                    {
                        stored.remote = None;
                        stored.node = None;
                        cleared += 1;
                    }
                }
            }

            let new_digest = if cleared > 0 {
                Some(pdm_config::subscriptions::save_config(&config)?)
            } else {
                None
            };
            Ok((cleared, new_digest))
        })
        .await??;

    if let Some(new_digest) = new_digest_opt {
        rpcenv["digest"] = new_digest.to_hex().into();
    }

    Ok(ClearPendingResult { cleared })
}

/// Plan entry for one pending push.
#[derive(Clone, Debug)]
struct PendingEntry {
    key: String,
    remote: String,
    node: String,
}

fn compute_pending(
    user_info: &CachedUserInfo,
    auth_id: &Authid,
    node_statuses: &[RemoteNodeStatus],
) -> Result<Vec<PendingEntry>, Error> {
    let (config, _) = pdm_config::subscriptions::config()?;

    Ok(config
        .iter()
        .filter_map(|(_id, entry)| {
            let remote = entry.remote.as_deref()?;
            let node = entry.node.as_deref()?;

            if user_info.lookup_privs(auth_id, &["resource", remote]) & PRIV_RESOURCE_MODIFY == 0 {
                return None;
            }

            // Pending push = the live current key on the node does not match the assigned pool
            // key. Subscription health (Invalid, Expired, ...) is a separate axis surfaced via
            // the Status column; re-pushing the same key would not change the shop's verdict.
            // Unreachable remotes count as pending so a stuck assignment can still be cleared
            // without first having to bring the target back online.
            let is_pending = match node_statuses
                .iter()
                .find(|n| n.remote == remote && n.node == node)
            {
                Some(n) => n.current_key.as_deref() != Some(entry.key.as_str()),
                None => true,
            };

            is_pending.then(|| PendingEntry {
                key: entry.key.clone(),
                remote: remote.to_string(),
                node: node.to_string(),
            })
        })
        .collect())
}

fn pool_assignment_still_valid(
    config: &SectionConfigData<SubscriptionKeyEntry>,
    entry: &PendingEntry,
) -> bool {
    let Some(stored) = config.get(&entry.key) else {
        return false;
    };
    stored.remote.as_deref() == Some(entry.remote.as_str())
        && stored.node.as_deref() == Some(entry.node.as_str())
}

/// Like [`collect_node_status`] but bypasses the auth filter, for the apply-pending worker
/// which gates each entry through its own per-remote priv check based on the persisted pool plan.
async fn collect_status_uncached(
    remotes_config: &SectionConfigData<Remote>,
) -> Vec<RemoteNodeStatus> {
    let fetch = remotes_config.iter().map(|(name, remote)| async move {
        let res = get_subscription_info_for_remote(remote, FRESH_NODE_STATUS_MAX_AGE).await;
        (name.to_string(), remote.ty, res)
    });
    let results = join_all(fetch).await;

    let mut out = Vec::new();
    for (remote_name, remote_ty, result) in results {
        let Ok(node_infos) = result else { continue };
        for (node_name, node_info) in &node_infos {
            let (status, level, sockets, current_key) = match node_info {
                Some(info) => (info.status, info.level, info.sockets, info.key.clone()),
                None => (
                    proxmox_subscription::SubscriptionStatus::NotFound,
                    SubscriptionLevel::None,
                    None,
                    None,
                ),
            };
            out.push(RemoteNodeStatus {
                remote: remote_name.clone(),
                ty: remote_ty,
                node: node_name.to_string(),
                sockets,
                status,
                level,
                assigned_key: None,
                current_key,
                pending_clear: false,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdm_api_types::remotes::RemoteType;
    use pdm_api_types::subscription::SubscriptionKeyEntry;
    use proxmox_subscription::SubscriptionStatus;

    #[test]
    fn redact_key_handles_standard_pve_key() {
        assert_eq!(redact_key("pve4b-1234567890"), "pve4b-1...0");
    }

    #[test]
    fn redact_key_handles_standard_pbs_key() {
        assert_eq!(redact_key("pbsc-abcdef0123"), "pbsc-a...3");
    }

    #[test]
    fn redact_key_safe_on_single_char_secret() {
        assert_eq!(redact_key("pve4b-x"), "pve4b-x...");
    }

    #[test]
    fn redact_key_safe_on_empty_secret() {
        assert_eq!(redact_key("pve4b-"), "pve4b-...");
    }

    #[test]
    fn redact_key_malformed_no_dash() {
        assert_eq!(redact_key("nodash"), "<malformed-key>");
    }

    fn pool_entry(key: &str, remote: Option<&str>, node: Option<&str>) -> SubscriptionKeyEntry {
        SubscriptionKeyEntry {
            key: key.to_string(),
            product_type: ProductType::from_key(key).unwrap_or_default(),
            level: SubscriptionLevel::from_key(Some(key)),
            source: SubscriptionKeySource::Manual,
            remote: remote.map(str::to_string),
            node: node.map(str::to_string),
            ..Default::default()
        }
    }

    fn pool_config(entries: Vec<SubscriptionKeyEntry>) -> SectionConfigData<SubscriptionKeyEntry> {
        let mut data = SectionConfigData::default();
        for e in entries {
            data.insert(e.key.clone(), e);
        }
        data
    }

    fn node_status(
        remote: &str,
        node: &str,
        ty: RemoteType,
        sockets: Option<i64>,
    ) -> RemoteNodeStatus {
        RemoteNodeStatus {
            remote: remote.to_string(),
            ty,
            node: node.to_string(),
            sockets,
            status: SubscriptionStatus::NotFound,
            level: SubscriptionLevel::None,
            assigned_key: None,
            current_key: None,
            pending_clear: false,
        }
    }

    #[test]
    fn compute_proposals_picks_smallest_covering_pve_key() {
        // Pool has a 1-socket, 2-socket, and 4-socket PVE key; the 2-socket target node should
        // take the 2-socket key (smallest covering), not the 4-socket one.
        let config = pool_config(vec![
            pool_entry("pve1b-1111111111", None, None),
            pool_entry("pve2b-2222222222", None, None),
            pool_entry("pve4b-4444444444", None, None),
        ]);
        let statuses = vec![node_status("remote1", "node1", RemoteType::Pve, Some(2))];
        let proposals = compute_proposals(&config, &statuses);
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].key, "pve2b-2222222222");
        assert_eq!(proposals[0].remote, "remote1");
        assert_eq!(proposals[0].node, "node1");
    }

    #[test]
    fn compute_proposals_skips_active_nodes() {
        let config = pool_config(vec![pool_entry("pve2b-2222222222", None, None)]);
        let mut active = node_status("remote1", "node1", RemoteType::Pve, Some(2));
        active.status = SubscriptionStatus::Active;
        let proposals = compute_proposals(&config, &[active]);
        assert!(proposals.is_empty());
    }

    #[test]
    fn compute_proposals_no_double_assignment() {
        // Two nodes compete for one pool key; only one should be assigned.
        let config = pool_config(vec![pool_entry("pve2b-2222222222", None, None)]);
        let statuses = vec![
            node_status("remote1", "node1", RemoteType::Pve, Some(2)),
            node_status("remote1", "node2", RemoteType::Pve, Some(2)),
        ];
        let proposals = compute_proposals(&config, &statuses);
        assert_eq!(proposals.len(), 1);
    }

    #[test]
    fn compute_proposals_pbs_picks_first_candidate() {
        // PBS keys have no socket count; the first matching candidate wins.
        let config = pool_config(vec![pool_entry("pbsc-1111111111", None, None)]);
        let statuses = vec![node_status("remote1", "node1", RemoteType::Pbs, None)];
        let proposals = compute_proposals(&config, &statuses);
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].key, "pbsc-1111111111");
    }

    #[test]
    fn hash_node_status_stable_across_input_order() {
        let a = node_status("r1", "n1", RemoteType::Pve, Some(2));
        let b = node_status("r2", "n2", RemoteType::Pve, Some(4));
        let h1 = hash_node_status(&[a.clone(), b.clone()]);
        let h2 = hash_node_status(&[b, a]);
        assert_eq!(h1, h2, "hash must be order-independent");
    }

    #[test]
    fn hash_node_status_changes_with_status() {
        let mut a = node_status("r1", "n1", RemoteType::Pve, Some(2));
        let before = hash_node_status(&[a.clone()]);
        a.status = SubscriptionStatus::Active;
        let after = hash_node_status(&[a]);
        assert_ne!(before, after, "hash must reflect status changes");
    }

    #[test]
    fn hash_node_status_changes_with_assigned_key_presence() {
        let mut a = node_status("r1", "n1", RemoteType::Pve, Some(2));
        let before = hash_node_status(&[a.clone()]);
        a.assigned_key = Some("pve2b-1234567890".to_string());
        let after = hash_node_status(&[a]);
        assert_ne!(
            before, after,
            "hash must reflect assigned_key presence (gates the auto-assign apply window)"
        );
    }
}
