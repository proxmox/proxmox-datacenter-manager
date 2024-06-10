use ::serde::{Deserialize, Serialize};
use anyhow::Error;
use pdm_api_types::ConfigDigest;

use proxmox_router::{Permission, Router, RpcEnvironment};
use proxmox_schema::api;

use pdm_api_types::{PRIV_SYS_AUDIT, PRIV_SYS_MODIFY};

//use crate::api2::node::apt::update_apt_proxy_config;
use pdm_config::certificate_config::AcmeCertificateConfig;

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_GET_CERTIFICATE_CONFIG)
    .put(&API_METHOD_UPDATE_CERTIFICATE_CONFIG);

#[api(
    access: {
        permission: &Permission::Privilege(&["system"], PRIV_SYS_AUDIT, false),
    },
    returns: {
        type: AcmeCertificateConfig,
    },
    protected: true,
)]
/// Get the node configuration
pub fn get_certificate_config(
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<AcmeCertificateConfig, Error> {
    let (config, digest) = pdm_config::certificate_config::config()?;
    rpcenv["digest"] = hex::encode(digest).into();
    Ok(config)
}

#[api()]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
/// Deletable property name
pub enum DeletableProperty {
    /// Delete the acme property.
    Acme,
    /// Delete the acmedomain0 property.
    Acmedomain0,
    /// Delete the acmedomain1 property.
    Acmedomain1,
    /// Delete the acmedomain2 property.
    Acmedomain2,
    /// Delete the acmedomain3 property.
    Acmedomain3,
    /// Delete the acmedomain4 property.
    Acmedomain4,
}

#[api(
    input: {
        properties: {
            digest: {
                type: ConfigDigest,
                optional: true,
            },
            update: {
                type: AcmeCertificateConfig,
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
        },
    },
    access: {
        permission: &Permission::Privilege(&["system"], PRIV_SYS_MODIFY, false),
    },
    protected: true,
)]
/// Update the node configuration
pub fn update_certificate_config(
    update: AcmeCertificateConfig,
    delete: Option<Vec<DeletableProperty>>,
    digest: Option<ConfigDigest>,
) -> Result<(), Error> {
    let _lock = pdm_config::certificate_config::lock()?;
    let (mut config, expected_digest) = pdm_config::certificate_config::config()?;

    // FIXME: GUI doesn't handle our non-inlined digest part here properly...
    expected_digest.detect_modification(digest.as_ref())?;

    if let Some(delete) = delete {
        for delete_prop in delete {
            match delete_prop {
                DeletableProperty::Acme => {
                    config.acme = None;
                }
                DeletableProperty::Acmedomain0 => {
                    config.acmedomain0 = None;
                }
                DeletableProperty::Acmedomain1 => {
                    config.acmedomain1 = None;
                }
                DeletableProperty::Acmedomain2 => {
                    config.acmedomain2 = None;
                }
                DeletableProperty::Acmedomain3 => {
                    config.acmedomain3 = None;
                }
                DeletableProperty::Acmedomain4 => {
                    config.acmedomain4 = None;
                }
            }
        }
    }

    if update.acme.is_some() {
        config.acme = update.acme;
    }
    if update.acmedomain0.is_some() {
        config.acmedomain0 = update.acmedomain0;
    }
    if update.acmedomain1.is_some() {
        config.acmedomain1 = update.acmedomain1;
    }
    if update.acmedomain2.is_some() {
        config.acmedomain2 = update.acmedomain2;
    }
    if update.acmedomain3.is_some() {
        config.acmedomain3 = update.acmedomain3;
    }
    if update.acmedomain4.is_some() {
        config.acmedomain4 = update.acmedomain4;
    }

    pdm_config::certificate_config::save_config(&config)?;

    // fixme: update_apt_proxy_config(config.http_proxy().as_ref())?;

    Ok(())
}
