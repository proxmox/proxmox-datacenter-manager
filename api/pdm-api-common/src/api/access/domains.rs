//! List Authentication domains/realms.

use anyhow::Error;
use serde::{Deserialize, Serialize};

use proxmox_router::{Permission, Router};
use proxmox_schema::api;

#[api]
#[derive(Clone, PartialEq, Deserialize, Serialize)]
/// Realm information.
struct BasicRealmInfo {
    /// Realm name.
    realm: String,

    /// Realm type.
    #[serde(rename = "type")]
    ty: String,

    /// Comment.
    #[serde(skip_serializing_if = "Option::is_none")]
    comment: Option<String>,
}

#[api(
    returns: {
        description: "List of realms with basic info.",
        type: Array,
        items: {
            type: BasicRealmInfo,

        },
    },
    access: {
        description: "Anyone can access this, because we need that list for the login box (before the user is authenticated).",
        permission: &Permission::World,
    },
)]
/// Authentication domain/realm index.
fn list_domains() -> Result<Vec<BasicRealmInfo>, Error> {
    Ok(vec![
        BasicRealmInfo {
            realm: "pam".to_string(),
            ty: "pam".to_string(),
            comment: Some("Linux PAM standard authentication".to_string()),
        },
        BasicRealmInfo {
            realm: "pdm".to_string(),
            ty: "pdm".to_string(),
            comment: Some("Proxmox Datacenter Manager authentication".to_string()),
        },
    ])
}

pub const ROUTER: Router = Router::new().get(&API_METHOD_LIST_DOMAINS);
