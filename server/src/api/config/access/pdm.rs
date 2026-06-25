use anyhow::Error;
use serde::{Deserialize, Serialize};

use proxmox_config_digest::ConfigDigest;
use proxmox_router::{Permission, Router, RpcEnvironment};
use proxmox_schema::api;

use pdm_api_types::{PRIV_REALM_ALLOCATE, PRIV_SYS_AUDIT, PdmRealmConfig, PdmRealmConfigUpdater};
use pdm_config::domains;

#[api(
    returns: {
        type: PdmRealmConfig,
    },
    access: {
        permission: &Permission::Privilege(&["access", "domains"], PRIV_SYS_AUDIT, false),
    },
)]
/// Read the Proxmox Datacenter Manager authentication server realm configuration
pub fn read_pdm_realm(rpcenv: &mut dyn RpcEnvironment) -> Result<PdmRealmConfig, Error> {
    let (domains, digest) = domains::config()?;

    let config = domains.lookup("pdm", "pdm")?;

    rpcenv["digest"] = digest.to_hex().into();

    Ok(config)
}

#[api]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
/// Deletable property name
pub enum DeletableProperty {
    /// Delete the comment property.
    Comment,
    /// Delete the default property.
    Default,
}

#[api(
    protected: true,
    input: {
        properties: {
            update: {
                type: PdmRealmConfigUpdater,
                flatten: true,
            },
            delete: {
                description: "List of properties to delete.",
                type: Array,
                optional: true,
                items: {
                    type: DeletableProperty,
                }
            },
            digest: {
                optional: true,
                type: ConfigDigest,
            },
        },
    },
    returns: {
        type: PdmRealmConfig,
    },
    access: {
        permission: &Permission::Privilege(&["access", "domains"], PRIV_REALM_ALLOCATE, false),
    },
)]
/// Update the Proxmox Datacenter Manager authentication server realm configuration
pub fn update_pdm_realm(
    update: PdmRealmConfigUpdater,
    delete: Option<Vec<DeletableProperty>>,
    digest: Option<ConfigDigest>,
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<(), Error> {
    let _lock = domains::lock_config()?;

    let (mut domains, expected_digest) = domains::config()?;

    expected_digest.detect_modification(digest.as_ref())?;

    let mut config: PdmRealmConfig = domains.lookup("pdm", "pdm")?;

    if let Some(delete) = delete {
        for delete_prop in delete {
            match delete_prop {
                DeletableProperty::Comment => config.comment = None,
                DeletableProperty::Default => config.default = None,
            }
        }
    }

    if let Some(comment) = update.comment {
        let comment = comment.trim().to_string();
        if comment.is_empty() {
            config.comment = None;
        } else {
            config.comment = Some(comment);
        }
    }

    if let Some(true) = update.default {
        pdm_config::domains::unset_default_realm(&mut domains)?;
        config.default = Some(true);
    } else {
        config.default = None;
    }

    domains.set_data("pdm", "pdm", &config)?;

    domains::save_config(&domains)?;

    Ok(())
}

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_READ_PDM_REALM)
    .put(&API_METHOD_UPDATE_PDM_REALM);
