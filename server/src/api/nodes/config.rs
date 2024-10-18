use ::serde::{Deserialize, Serialize};
use anyhow::Error;

use proxmox_router::{Permission, Router, RpcEnvironment};
use proxmox_schema::api;

use pdm_api_types::{ConfigDigest, NODE_SCHEMA, PRIV_SYS_AUDIT, PRIV_SYS_MODIFY};

use crate::api::nodes::apt::update_apt_proxy_config;
use pdm_api_types::{NodeConfig, NodeConfigUpdater};

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_GET_NODE_CONFIG)
    .put(&API_METHOD_UPDATE_NODE_CONFIG);

#[api(
    input: {
        properties: {
            node: { schema: NODE_SCHEMA },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system"], PRIV_SYS_AUDIT, false),
    },
    returns: {
        type: NodeConfig,
    },
)]
/// Get the node configuration
pub fn get_node_config(rpcenv: &mut dyn RpcEnvironment) -> Result<NodeConfig, Error> {
    let (config, digest) = pdm_config::node::config()?;
    rpcenv["digest"] = hex::encode(digest).into();
    Ok(config)
}

#[api()]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
/// Deletable property name
pub enum DeletableProperty {
    /// Delete the http-proxy property.
    HttpProxy,
    /// Delete the email-from property.
    EmailFrom,
    /// Delete the ciphers-tls-1.3 property.
    #[serde(rename = "ciphers-tls-1.3")]
    CiphersTls1_3,
    /// Delete the ciphers-tls-1.2 property.
    #[serde(rename = "ciphers-tls-1.2")]
    CiphersTls1_2,
    /// Delete the default-lang property.
    DefaultLang,
}

#[api(
    input: {
        properties: {
            node: { schema: NODE_SCHEMA },
            digest: {
                type: ConfigDigest,
                optional: true,
            },
            update: {
                type: NodeConfigUpdater,
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
pub fn update_node_config(
    // node: String, // not used
    update: NodeConfigUpdater,
    delete: Option<Vec<DeletableProperty>>,
    digest: Option<ConfigDigest>,
) -> Result<(), Error> {
    let _lock = pdm_config::node::lock()?;
    let (mut config, expected_digest) = pdm_config::node::config()?;
    expected_digest.detect_modification(digest.as_ref())?;

    if let Some(delete) = delete {
        for delete_prop in delete {
            match delete_prop {
                DeletableProperty::HttpProxy => {
                    config.http_proxy = None;
                }
                DeletableProperty::EmailFrom => {
                    config.email_from = None;
                }
                DeletableProperty::CiphersTls1_3 => {
                    config.ciphers_tls_1_3 = None;
                }
                DeletableProperty::CiphersTls1_2 => {
                    config.ciphers_tls_1_2 = None;
                }
                DeletableProperty::DefaultLang => {
                    config.default_lang = None;
                }
            }
        }
    }

    if update.http_proxy.is_some() {
        config.http_proxy = update.http_proxy;
    }
    if update.email_from.is_some() {
        config.email_from = update.email_from;
    }
    if update.ciphers_tls_1_3.is_some() {
        config.ciphers_tls_1_3 = update.ciphers_tls_1_3;
    }
    if update.ciphers_tls_1_2.is_some() {
        config.ciphers_tls_1_2 = update.ciphers_tls_1_2;
    }
    if update.default_lang.is_some() {
        config.default_lang = update.default_lang;
    }

    pdm_config::node::save_config(&config)?;

    update_apt_proxy_config(pdm_config::node::get_http_proxy_config(&config).as_ref())?;

    Ok(())
}
