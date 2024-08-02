use anyhow::{bail, Error};

use proxmox_access_control::acl::AclTreeNode;
use proxmox_access_control::CachedUserInfo;
use proxmox_config_digest::{ConfigDigest, PROXMOX_CONFIG_DIGEST_SCHEMA};
use proxmox_router::{Permission, Router, RpcEnvironment};
use proxmox_schema::api;

use pdm_api_types::{
    AclListItem, AclUgidType, Authid, Role, ACL_PATH_SCHEMA, ACL_PROPAGATE_SCHEMA,
    PRIV_ACCESS_AUDIT, PRIV_ACCESS_MODIFY, PROXMOX_GROUP_ID_SCHEMA,
};

pub(super) const ROUTER: Router = Router::new()
    .get(&API_METHOD_READ_ACL)
    .put(&API_METHOD_UPDATE_ACL);

// FIXME: copied from PBS
fn extract_acl_node_data(
    node: &AclTreeNode,
    path: &str,
    list: &mut Vec<AclListItem>,
    exact: bool,
    auth_id_filter: &Option<Authid>,
) {
    // tokens can't have tokens, so we can early return
    if let Some(auth_id_filter) = auth_id_filter {
        if auth_id_filter.is_token() {
            return;
        }
    }

    for (user, roles) in &node.users {
        if let Some(auth_id_filter) = auth_id_filter {
            if !user.is_token() || user.user() != auth_id_filter.user() {
                continue;
            }
        }

        for (role, propagate) in roles {
            list.push(AclListItem {
                path: if path.is_empty() {
                    String::from("/")
                } else {
                    path.to_string()
                },
                propagate: *propagate,
                ugid_type: AclUgidType::User,
                ugid: user.to_string(),
                roleid: role.to_string(),
            });
        }
    }
    for (group, roles) in &node.groups {
        if auth_id_filter.is_some() {
            continue;
        }

        for (role, propagate) in roles {
            list.push(AclListItem {
                path: if path.is_empty() {
                    String::from("/")
                } else {
                    path.to_string()
                },
                propagate: *propagate,
                ugid_type: AclUgidType::Group,
                ugid: group.to_string(),
                roleid: role.to_string(),
            });
        }
    }
    if exact {
        return;
    }
    for (comp, child) in &node.children {
        let new_path = format!("{}/{}", path, comp);
        extract_acl_node_data(child, &new_path, list, exact, auth_id_filter);
    }
}

#[api(
    input: {
        properties: {
            path: {
                schema: ACL_PATH_SCHEMA,
                optional: true,
            },
            exact: {
                description: "If set, returns only ACL for the exact path.",
                type: bool,
                optional: true,
                default: false,
            },
        },
    },
    returns: {
        description: "ACL entry list.",
        type: Array,
        items: {
            type: AclListItem,
        }
    },
    access: {
        permission: &Permission::Anybody,
        description: "Returns all ACLs if user has Sys.Audit on '/access/acl', or just the ACLs containing the user's API tokens.",
    },
)]
/// Read Access Control List (ACLs).
fn read_acl(
    path: Option<String>,
    exact: bool,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<AclListItem>, Error> {
    let auth_id = rpcenv.get_auth_id().unwrap().parse()?;

    let user_info = CachedUserInfo::new()?;

    let top_level_privs = user_info.lookup_privs(&auth_id, &["access", "acl"]);
    let auth_id_filter = if (top_level_privs & PRIV_ACCESS_AUDIT) == 0 {
        Some(auth_id)
    } else {
        None
    };

    let (mut tree, digest) = proxmox_access_control::acl::config()?;

    let mut list: Vec<AclListItem> = Vec::new();
    if let Some(path) = &path {
        if let Some(node) = &tree.find_node(path) {
            extract_acl_node_data(node, path, &mut list, exact, &auth_id_filter);
        }
    } else {
        extract_acl_node_data(&tree.root, "", &mut list, exact, &auth_id_filter);
    }

    rpcenv["digest"] = hex::encode(digest).into();

    Ok(list)
}

#[api(
    protected: true,
    input: {
        properties: {
            path: {
                schema: ACL_PATH_SCHEMA,
            },
            role: {
                type: Role,
            },
            propagate: {
                optional: true,
                schema: ACL_PROPAGATE_SCHEMA,
            },
            "auth-id": {
                optional: true,
                type: Authid,
            },
            group: {
                optional: true,
                schema: PROXMOX_GROUP_ID_SCHEMA,
            },
            delete: {
                optional: true,
                description: "Remove permissions (instead of adding it).",
                type: bool,
                default: false,
            },
            digest: {
                optional: true,
                schema: PROXMOX_CONFIG_DIGEST_SCHEMA,
            },
       },
    },
    access: {
        permission: &Permission::Anybody,
        description: "Requires Permissions.Modify on '/access/acl', limited to updating ACLs of the user's API tokens otherwise."
    },
)]
/// Update Access Control List (ACLs).
#[allow(clippy::too_many_arguments)]
fn update_acl(
    path: String,
    role: String,
    propagate: Option<bool>,
    auth_id: Option<Authid>,
    group: Option<String>,
    delete: bool,
    digest: Option<ConfigDigest>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<(), Error> {
    let current_auth_id: Authid = rpcenv.get_auth_id().unwrap().parse()?;

    let user_info = CachedUserInfo::new()?;

    let top_level_privs = user_info.lookup_privs(&current_auth_id, &["access", "acl"]);
    if top_level_privs & PRIV_ACCESS_MODIFY == 0 {
        if group.is_some() {
            bail!("Unprivileged users are not allowed to create group ACL item.");
        }

        match &auth_id {
            Some(auth_id) => {
                if current_auth_id.is_token() {
                    bail!("Unprivileged API tokens can't set ACL items.");
                } else if !auth_id.is_token() {
                    bail!("Unprivileged users can only set ACL items for API tokens.");
                } else if auth_id.user() != current_auth_id.user() {
                    bail!("Unprivileged users can only set ACL items for their own API tokens.");
                }
            }
            None => {
                bail!("Unprivileged user needs to provide auth_id to update ACL item.");
            }
        };
    }

    let _lock = proxmox_access_control::acl::lock_config()?;

    let (mut tree, expected_digest) = proxmox_access_control::acl::config()?;

    expected_digest.detect_modification(digest.as_ref())?;

    let propagate = propagate.unwrap_or(true);

    if let Some(ref _group) = group {
        bail!("parameter 'group' - groups are currently not supported.");
    } else if let Some(ref auth_id) = auth_id {
        if !delete {
            // Note: we allow to delete non-existent users
            let user_cfg = proxmox_access_control::user::cached_config()?;
            if !user_cfg.sections.contains_key(&auth_id.to_string()) {
                bail!(format!(
                    "no such {}.",
                    if auth_id.is_token() {
                        "API token"
                    } else {
                        "user"
                    }
                ));
            }
        }
    } else {
        bail!("missing 'userid' or 'group' parameter.");
    }

    if !delete {
        // Note: we allow to delete entries with invalid path
        check_acl_path(&path)?;
    }

    if let Some(auth_id) = auth_id {
        if delete {
            tree.delete_user_role(&path, &auth_id, &role);
        } else {
            tree.insert_user_role(&path, &auth_id, &role, propagate);
        }
    } else if let Some(group) = group {
        if delete {
            tree.delete_group_role(&path, &group, &role);
        } else {
            tree.insert_group_role(&path, &group, &role, propagate);
        }
    }

    proxmox_access_control::acl::save_config(&tree)?;

    Ok(())
}
///
/// Check whether a given ACL `path` conforms to the expected schema.
///
/// Currently this just checks for the number of components for various sub-trees.
fn check_acl_path(path: &str) -> Result<(), Error> {
    let components = proxmox_access_control::acl::split_acl_path(path);

    let components_len = components.len();

    if components_len == 0 {
        return Ok(());
    }
    match components[0] {
        "access" => {
            if components_len == 1 {
                return Ok(());
            }
            match components[1] {
                "acl" | "users" | "realm" => {
                    if components_len == 2 {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
        "resource" => {
            // `/resource` and `/resource/{remote}`
            if components_len <= 2 {
                return Ok(());
            }
            // `/resource/{remote-id}/{resource-type=guest,storage}/{resource-id}`
            match components[2] {
                "guest" | "storage" => {
                    // /resource/{remote-id}/{resource-type}
                    // /resource/{remote-id}/{resource-type}/{resource-id}
                    if components_len <= 4 {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
        "system" => {
            if components_len == 1 {
                return Ok(());
            }
            match components[1] {
                "certificates" | "disks" | "log" | "notifications" | "status" | "tasks"
                | "time" => {
                    if components_len == 2 {
                        return Ok(());
                    }
                }
                "services" => {
                    // /system/services/{service}
                    if components_len <= 3 {
                        return Ok(());
                    }
                }
                "network" => {
                    if components_len == 2 {
                        return Ok(());
                    }
                    match components[2] {
                        "dns" => {
                            if components_len == 3 {
                                return Ok(());
                            }
                        }
                        "interfaces" => {
                            // /system/network/interfaces/{iface}
                            if components_len <= 4 {
                                return Ok(());
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }

    bail!("invalid acl path '{}'.", path);
}
