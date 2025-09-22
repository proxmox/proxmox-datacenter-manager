use anyhow::{format_err, Error};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use proxmox_config_digest::ConfigDigest;
use proxmox_ldap::types::{LdapRealmConfig, LdapRealmConfigUpdater, REALM_ID_SCHEMA};
use proxmox_ldap::Connection;
use proxmox_router::{http_bail, Permission, Router, RpcEnvironment};
use proxmox_schema::{api, param_bail};

use pdm_api_types::{PRIV_REALM_ALLOCATE, PRIV_SYS_AUDIT};
use pdm_config::domains;

use crate::auth::ldap;
use crate::auth::ldap::LdapAuthenticator;

#[api(
    input: {
        properties: {},
    },
    returns: {
        description: "List of configured LDAP realms.",
        type: Array,
        items: { type: LdapRealmConfig },
    },
    access: {
        permission: &Permission::Privilege(&["access", "domains"], PRIV_REALM_ALLOCATE, false),
    },
)]
/// List configured LDAP realms
pub fn list_ldap_realms(
    _param: Value,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<LdapRealmConfig>, Error> {
    let (config, digest) = domains::config()?;

    let list = config.convert_to_typed_array("ldap")?;

    rpcenv["digest"] = digest.to_hex().into();

    Ok(list)
}

#[api(
    protected: true,
    input: {
        properties: {
            config: {
                type: LdapRealmConfig,
                flatten: true,
            },
            password: {
                description: "LDAP bind password",
                optional: true,
            }
        },
    },
    access: {
        permission: &Permission::Privilege(&["access", "domains"], PRIV_REALM_ALLOCATE, false),
    },
)]
/// Create a new LDAP realm
pub fn create_ldap_realm(config: LdapRealmConfig, password: Option<String>) -> Result<(), Error> {
    let domain_config_lock = domains::lock_config()?;

    let (mut domains, _digest) = domains::config()?;

    if domains::exists(&domains, &config.realm) {
        param_bail!("realm", "realm '{}' already exists.", config.realm);
    }

    let ldap_config =
        LdapAuthenticator::api_type_to_config_with_password(&config, password.clone())?;

    let conn = Connection::new(ldap_config);
    proxmox_async::runtime::block_on(conn.check_connection()).map_err(|e| format_err!("{e:#}"))?;

    if let Some(password) = password {
        ldap::store_ldap_bind_password(&config.realm, &password, &domain_config_lock)?;
    }

    if let Some(true) = config.default {
        domains::unset_default_realm(&mut domains)?;
    }

    domains.set_data(&config.realm, "ldap", &config)?;

    domains::save_config(&domains)
}

#[api(
    protected: true,
    input: {
        properties: {
            realm: {
                schema: REALM_ID_SCHEMA,
            },
            digest: {
                optional: true,
                type: ConfigDigest,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["access", "domains"], PRIV_REALM_ALLOCATE, false),
    },
)]
/// Remove an LDAP realm configuration
pub fn delete_ldap_realm(
    realm: String,
    digest: Option<ConfigDigest>,
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<(), Error> {
    let domain_config_lock = domains::lock_config()?;

    let (mut domains, expected_digest) = domains::config()?;
    expected_digest.detect_modification(digest.as_ref())?;

    if domains.sections.remove(&realm).is_none() {
        http_bail!(NOT_FOUND, "realm '{realm}' does not exist.");
    }

    domains::save_config(&domains)?;

    if ldap::remove_ldap_bind_password(&realm, &domain_config_lock).is_err() {
        log::error!("Could not remove stored LDAP bind password for realm {realm}");
    }

    Ok(())
}

#[api(
    input: {
        properties: {
            realm: {
                schema: REALM_ID_SCHEMA,
            },
        },
    },
    returns:  { type: LdapRealmConfig },
    access: {
        permission: &Permission::Privilege(&["access", "domains"], PRIV_SYS_AUDIT, false),
    },
)]
/// Read the LDAP realm configuration
pub fn read_ldap_realm(
    realm: String,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<LdapRealmConfig, Error> {
    let (domains, digest) = domains::config()?;

    let config = domains.lookup("ldap", &realm)?;

    rpcenv["digest"] = digest.to_hex().into();

    Ok(config)
}

#[api()]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
/// Deletable property name
pub enum DeletableProperty {
    /// Fallback LDAP server address
    Server2,
    /// Port
    Port,
    /// Comment
    Comment,
    /// Is default realm
    Default,
    /// Verify server certificate
    Verify,
    /// Mode (ldap, ldap+starttls or ldaps),
    Mode,
    /// Bind Domain
    BindDn,
    /// LDAP bind password
    Password,
    /// User filter
    Filter,
    /// Default options for user sync
    SyncDefaultsOptions,
    /// user attributes to sync with LDAP attributes
    SyncAttributes,
    /// User classes
    UserClasses,
}

#[api(
    protected: true,
    input: {
        properties: {
            realm: {
                schema: REALM_ID_SCHEMA,
            },
            update: {
                type: LdapRealmConfigUpdater,
                flatten: true,
            },
            password: {
                description: "LDAP bind password",
                optional: true,
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
    returns:  { type: LdapRealmConfig },
    access: {
        permission: &Permission::Privilege(&["access", "domains"], PRIV_REALM_ALLOCATE, false),
    },
)]
/// Update an LDAP realm configuration
pub fn update_ldap_realm(
    realm: String,
    update: LdapRealmConfigUpdater,
    password: Option<String>,
    delete: Option<Vec<DeletableProperty>>,
    digest: Option<ConfigDigest>,
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<(), Error> {
    let domain_config_lock = domains::lock_config()?;

    let (mut domains, expected_digest) = domains::config()?;
    expected_digest.detect_modification(digest.as_ref())?;

    let mut config: LdapRealmConfig = domains.lookup("ldap", &realm)?;

    if let Some(delete) = delete {
        for delete_prop in delete {
            match delete_prop {
                DeletableProperty::Server2 => {
                    config.server2 = None;
                }
                DeletableProperty::Comment => {
                    config.comment = None;
                }
                DeletableProperty::Default => {
                    config.default = None;
                }
                DeletableProperty::Port => {
                    config.port = None;
                }
                DeletableProperty::Verify => {
                    config.verify = None;
                }
                DeletableProperty::Mode => {
                    config.mode = None;
                }
                DeletableProperty::BindDn => {
                    config.bind_dn = None;
                }
                DeletableProperty::Password => {
                    ldap::remove_ldap_bind_password(&realm, &domain_config_lock)?;
                }
                DeletableProperty::Filter => {
                    config.filter = None;
                }
                DeletableProperty::SyncDefaultsOptions => {
                    config.sync_defaults_options = None;
                }
                DeletableProperty::SyncAttributes => {
                    config.sync_attributes = None;
                }
                DeletableProperty::UserClasses => {
                    config.user_classes = None;
                }
            }
        }
    }

    if let Some(server1) = update.server1 {
        config.server1 = server1;
    }

    if let Some(server2) = update.server2 {
        config.server2 = Some(server2);
    }

    if let Some(port) = update.port {
        config.port = Some(port);
    }

    if let Some(base_dn) = update.base_dn {
        config.base_dn = base_dn;
    }

    if let Some(user_attr) = update.user_attr {
        config.user_attr = user_attr;
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
        domains::unset_default_realm(&mut domains)?;
        config.default = Some(true);
    } else {
        config.default = None;
    }

    if let Some(mode) = update.mode {
        config.mode = Some(mode);
    }

    if let Some(verify) = update.verify {
        config.verify = Some(verify);
    }

    if let Some(bind_dn) = update.bind_dn {
        config.bind_dn = Some(bind_dn);
    }

    if let Some(filter) = update.filter {
        config.filter = Some(filter);
    }
    if let Some(sync_defaults_options) = update.sync_defaults_options {
        config.sync_defaults_options = Some(sync_defaults_options);
    }
    if let Some(sync_attributes) = update.sync_attributes {
        config.sync_attributes = Some(sync_attributes);
    }
    if let Some(user_classes) = update.user_classes {
        config.user_classes = Some(user_classes);
    }

    let ldap_config = if password.is_some() {
        LdapAuthenticator::api_type_to_config_with_password(&config, password.clone())?
    } else {
        LdapAuthenticator::api_type_to_config(&config)?
    };

    let conn = Connection::new(ldap_config);
    proxmox_async::runtime::block_on(conn.check_connection()).map_err(|e| format_err!("{e:#}"))?;

    if let Some(password) = password {
        ldap::store_ldap_bind_password(&realm, &password, &domain_config_lock)?;
    }

    domains.set_data(&realm, "ldap", &config)?;

    domains::save_config(&domains)?;

    Ok(())
}

const ITEM_ROUTER: Router = Router::new()
    .get(&API_METHOD_READ_LDAP_REALM)
    .put(&API_METHOD_UPDATE_LDAP_REALM)
    .delete(&API_METHOD_DELETE_LDAP_REALM);

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_LDAP_REALMS)
    .post(&API_METHOD_CREATE_LDAP_REALM)
    .match_all("realm", &ITEM_ROUTER);
