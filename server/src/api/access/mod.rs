//! Common `/api2/*/access/ticket` router.

use std::collections::{HashMap, HashSet};

use anyhow::{bail, Error};

use proxmox_access_control::acl::AclTreeNode;
use proxmox_access_control::CachedUserInfo;
use proxmox_router::{list_subdirs_api_method, Router, SubdirMap};
use proxmox_router::{Permission, RpcEnvironment};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use pdm_api_types::{Authid, ACL_PATH_SCHEMA, PRIVILEGES, PRIV_ACCESS_AUDIT};

mod acl;
mod domains;
mod tfa;
mod users;

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("acl", &acl::ROUTER),
    ("domains", &domains::ROUTER),
    (
        "permissions",
        &Router::new().get(&API_METHOD_LIST_PERMISSIONS)
    ),
    ("tfa", &tfa::ROUTER),
    (
        "ticket",
        &Router::new()
            .post(&proxmox_auth_api::api::API_METHOD_CREATE_TICKET_HTTP_ONLY)
            .delete(&proxmox_auth_api::api::API_METHOD_LOGOUT),
    ),
    ("users", &users::ROUTER),
]);

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

/// Check if a user is allowed to list someone's permissions.
fn check_list_permissions(
    who: &Authid,
    whose: &Authid,
    user_info: &CachedUserInfo,
) -> Result<(), Error> {
    // Users can list their own permissions.
    if who == whose {
        return Ok(());
    }

    // Users can list permissions of their tokens:
    if whose.is_token() && &Authid::from(whose.user().clone()) == who {
        return Ok(());
    }

    let user_privs = user_info.lookup_privs(who, &["access"]);
    // Users with AUDIT on `/access` can list anyone's permissions.
    if user_privs & PRIV_ACCESS_AUDIT != 0 {
        return Ok(());
    }

    bail!("not allowed to list permissions of {}", whose);
}

#[api(
    input: {
        properties: {
            "auth-id": {
                type: Authid,
                optional: true,
            },
            path: {
                schema: ACL_PATH_SCHEMA,
                optional: true,
            },
        },
    },
    access: {
        permission: &Permission::Anybody,
        description: "Requires Sys.Audit on '/access', limited to own privileges otherwise.",
    },
    returns: {
        description: "Map of ACL path to Map of privilege to propagate bit",
        type: Object,
        properties: {},
        additional_properties: true,
    },
)]
/// List permissions of given or currently authenticated user / API token.
///
/// Optionally limited to specific path.
pub fn list_permissions(
    auth_id: Option<Authid>,
    path: Option<String>,
    rpcenv: &dyn RpcEnvironment,
) -> Result<HashMap<String, HashMap<&'static str, bool>>, Error> {
    let current_auth_id: Authid = rpcenv.get_auth_id().unwrap().parse()?;

    let user_info = CachedUserInfo::new()?;

    let auth_id = match auth_id {
        Some(auth_id) => {
            check_list_permissions(&current_auth_id, &auth_id, &user_info)?;
            auth_id
        }
        None => current_auth_id,
    };

    fn populate_acl_paths(
        mut paths: HashSet<String>,
        node: AclTreeNode,
        path: &str,
    ) -> HashSet<String> {
        for (sub_path, child_node) in node.children {
            let sub_path = format!("{}/{}", path, &sub_path);
            paths = populate_acl_paths(paths, child_node, &sub_path);
            paths.insert(sub_path);
        }
        paths
    }

    let paths = match path {
        Some(path) => {
            let mut paths = HashSet::new();
            paths.insert(path);
            paths
        }
        None => {
            let mut paths = HashSet::new();

            let (acl_tree, _) = proxmox_access_control::acl::config()?;
            paths = populate_acl_paths(paths, acl_tree.root, "");

            // default paths, returned even if no ACL exists
            paths.insert("/".to_string());
            paths.insert("/access".to_string());
            paths.insert("/resource".to_string());
            paths.insert("/system".to_string());

            paths
        }
    };

    Ok(paths
        .into_iter()
        // path -> { priv_name -> propagate bool }
        .filter_map(|path| {
            let split_path = proxmox_access_control::acl::split_acl_path(path.as_str());
            let (privs, propagated_privs) = user_info.lookup_privs_details(&auth_id, &split_path);

            if privs == 0 {
                None // Don't leak ACL paths where we don't have any privileges
            } else {
                // priv_name -> propagate
                let priv_map = PRIVILEGES
                    .iter()
                    .filter(|(_, value)| value & privs != 0)
                    .map(|(name, value)| (*name, value & propagated_privs != 0))
                    .collect();

                Some((path, priv_map))
            }
        })
        .collect())
}
