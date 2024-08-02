use anyhow::Error;

use proxmox_router::list_subdirs_api_method;
use proxmox_router::{Router, RpcEnvironment, SubdirMap};

use proxmox_schema::api;
use proxmox_sortable_macro::sortable;

use proxmox_acme_api::{
    AccountEntry, AccountInfo, AcmeAccountName, AcmeChallengeSchema, ChallengeSchemaWrapper,
    DeletablePluginProperty, DnsPluginCore, DnsPluginCoreUpdater, KnownAcmeDirectory, PluginConfig,
    PLUGIN_ID_SCHEMA,
};

use pdm_api_types::{ConfigDigest, PRIV_SYS_MODIFY};

use crate::api::Permission;

pub(crate) const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    (
        "account",
        &Router::new()
            .get(&API_METHOD_LIST_ACCOUNTS)
            .post(&API_METHOD_REGISTER_ACCOUNT)
            .match_all("name", &ACCOUNT_ITEM_ROUTER),
    ),
    (
        "plugins",
        &proxmox_router::Router::new()
            .get(&API_METHOD_LIST_PLUGINS)
            .post(&API_METHOD_ADD_PLUGIN)
            .match_all("id", &PLUGIN_ITEM_ROUTER),
    ),
    (
        "challenge-schema",
        &proxmox_router::Router::new().get(&API_METHOD_GET_CHALLENGE_SCHEMA),
    ),
    (
        "directories",
        &proxmox_router::Router::new().get(&API_METHOD_GET_DIRECTORIES),
    ),
    (
        "tos",
        &proxmox_router::Router::new().get(&API_METHOD_GET_TOS),
    ),
]);

const ACCOUNT_ITEM_ROUTER: Router = Router::new()
    .get(&API_METHOD_GET_ACCOUNT)
    .put(&API_METHOD_UPDATE_ACCOUNT)
    .delete(&API_METHOD_DEACTIVATE_ACCOUNT);

const PLUGIN_ITEM_ROUTER: proxmox_router::Router = proxmox_router::Router::new()
    .get(&API_METHOD_GET_PLUGIN)
    .put(&API_METHOD_UPDATE_PLUGIN)
    .delete(&API_METHOD_DELETE_PLUGIN);

#[api(
    access: {
        permission: &Permission::Privilege(&["system", "certificates"], PRIV_SYS_MODIFY, false),
    },
    returns: {
        type: Array,
        items: { type: AccountEntry },
        description: "List of ACME accounts.",
    },
    protected: true,
)]
/// List ACME accounts.
pub fn list_accounts() -> Result<Vec<AccountEntry>, Error> {
    proxmox_acme_api::list_accounts()
}

#[api(
    input: {
        properties: {
            name: {
                type: AcmeAccountName,
                optional: true,
            },
            contact: {
                description: "List of email addresses.",
            },
            tos_url: {
                description: "URL of CA TermsOfService - setting this indicates agreement.",
                optional: true,
            },
            directory: {
                type: String,
                description: "The ACME Directory.",
                optional: true,
            },
            eab_kid: {
                type: String,
                description: "Key Identifier for External Account Binding.",
                optional: true,
            },
            eab_hmac_key: {
                type: String,
                description: "HMAC Key for External Account Binding.",
                optional: true,
            }
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "certificates"], PRIV_SYS_MODIFY, false),
    },
    protected: true,
)]
/// Register an ACME account.
fn register_account(
    name: Option<AcmeAccountName>,
    // Todo: email & email-list schema
    contact: String,
    tos_url: Option<String>,
    directory: Option<String>,
    eab_kid: Option<String>,
    eab_hmac_key: Option<String>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<String, Error> {
    let auth_id = rpcenv.get_auth_id().unwrap();
    let name = name.unwrap_or_else(|| unsafe {
        AcmeAccountName::from_string_unchecked("default".to_string())
    });

    // TODO: this should be done via the api definition, but
    // the api schema currently lacks this ability (2023-11-06)
    if eab_kid.is_some() != eab_hmac_key.is_some() {
        proxmox_router::http_bail!(
            BAD_REQUEST,
            "either both or none of 'eab_kid' and 'eab_hmac_key' have to be set."
        );
    }

    let eab_cread = eab_kid.zip(eab_hmac_key);

    if std::path::Path::new(&proxmox_acme_api::account_config_filename(&name)).exists() {
        proxmox_router::http_bail!(BAD_REQUEST, "account {} already exists", name);
    }

    proxmox_rest_server::WorkerTask::spawn(
        "acme-register",
        Some(name.to_string()),
        auth_id,
        true,
        move |_worker| async move {
            proxmox_log::info!("Registering ACME account '{}'...", &name,);

            let location =
                proxmox_acme_api::register_account(&name, contact, tos_url, directory, eab_cread)
                    .await?;

            proxmox_log::info!("Registration successful, account URL: {}", location);

            Ok(())
        },
    )
}

#[api(
    input: {
        properties: {
            name: { type: AcmeAccountName },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "certificates"], PRIV_SYS_MODIFY, false),
    },
    returns: { type: AccountInfo },
    protected: true,
)]
/// Return existing ACME account information.
pub async fn get_account(name: AcmeAccountName) -> Result<AccountInfo, Error> {
    proxmox_acme_api::get_account(name).await
}

#[api(
    input: {
        properties: {
            name: { type: AcmeAccountName },
            contact: {
                description: "List of email addresses.",
                optional: true,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "certificates"], PRIV_SYS_MODIFY, false),
    },
    protected: true,
)]
/// Update an ACME account.
pub fn update_account(
    name: AcmeAccountName,
    // Todo: email & email-list schema
    contact: Option<String>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<String, Error> {
    let auth_id = rpcenv.get_auth_id().unwrap();

    proxmox_rest_server::WorkerTask::spawn(
        "acme-update",
        Some(name.to_string()),
        auth_id,
        true,
        move |_worker| async move {
            proxmox_log::info!("Update ACME account '{}'...", &name,);

            proxmox_acme_api::update_account(&name, contact).await?;

            proxmox_log::info!("Update ACME account '{}' successful", &name,);

            Ok(())
        },
    )
}

#[api(
    input: {
        properties: {
            name: { type: AcmeAccountName },
            force: {
                description:
                    "Delete account data even if the server refuses to deactivate the account.",
                optional: true,
                default: false,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "certificates"], PRIV_SYS_MODIFY, false),
    },
    protected: true,
)]
/// Deactivate an ACME account.
pub fn deactivate_account(
    name: AcmeAccountName,
    force: bool,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<String, Error> {
    let auth_id = rpcenv.get_auth_id().unwrap();

    proxmox_rest_server::WorkerTask::spawn(
        "acme-deactivate",
        Some(name.to_string()),
        auth_id,
        true,
        move |_worker| async move {
            proxmox_log::info!("Deactivate ACME account '{}'...", &name,);

            proxmox_acme_api::deactivate_account(&name, force).await?;

            proxmox_log::info!("Deactivate ACME account '{}' successful", &name,);

            Ok(())
        },
    )
}

#[api(
    access: {
        permission: &Permission::Privilege(&["system", "certificates"], PRIV_SYS_MODIFY, false),
    },
    protected: true,
    returns: {
        type: Array,
        description: "List of ACME plugin configurations.",
        items: { type: PluginConfig },
    },
)]
/// List ACME challenge plugins.
pub fn list_plugins(
    rpcenv: &mut dyn proxmox_router::RpcEnvironment,
) -> Result<Vec<PluginConfig>, Error> {
    proxmox_acme_api::list_plugins(rpcenv)
}

#[api(
    input: {
        properties: {
            id: { schema: PLUGIN_ID_SCHEMA },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "certificates"], PRIV_SYS_MODIFY, false),
    },
    protected: true,
    returns: { type: PluginConfig },
)]
/// List ACME challenge plugins.
pub fn get_plugin(
    id: String,
    rpcenv: &mut dyn proxmox_router::RpcEnvironment,
) -> Result<PluginConfig, Error> {
    proxmox_acme_api::get_plugin(id, rpcenv)
}

// Currently we only have "the" standalone plugin and DNS plugins so we can just flatten a
// DnsPluginUpdater:
//
// FIXME: The 'id' parameter should not be "optional" in the schema.
#[api(
    input: {
        properties: {
            type: {
                type: String,
                description: "The ACME challenge plugin type.",
            },
            core: {
                type: DnsPluginCore,
                flatten: true,
            },
            data: {
                type: String,
                // This is different in the API!
                description: "DNS plugin data (base64 encoded with padding).",
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "certificates"], PRIV_SYS_MODIFY, false),
    },
    protected: true,
)]
/// Add ACME plugin configuration.
pub fn add_plugin(r#type: String, core: DnsPluginCore, data: String) -> Result<(), Error> {
    proxmox_acme_api::add_plugin(r#type, core, data)
}

#[api(
    input: {
        properties: {
            id: { schema: PLUGIN_ID_SCHEMA },
            update: {
                type: DnsPluginCoreUpdater,
                flatten: true,
            },
            data: {
                type: String,
                optional: true,
                // This is different in the API!
                description: "DNS plugin data (base64 encoded with padding).",
            },
            delete: {
                description: "List of properties to delete.",
                type: Array,
                optional: true,
                items: {
                    type: DeletablePluginProperty,
                }
            },
            digest: {
                optional: true,
                type: ConfigDigest,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "certificates"], PRIV_SYS_MODIFY, false),
    },
    protected: true,
)]
/// Update an ACME plugin configuration.
pub fn update_plugin(
    id: String,
    update: DnsPluginCoreUpdater,
    data: Option<String>,
    delete: Option<Vec<DeletablePluginProperty>>,
    digest: Option<ConfigDigest>,
) -> Result<(), anyhow::Error> {
    proxmox_acme_api::update_plugin(id, update, data, delete, digest)
}

#[api(
    input: {
        properties: {
            id: { schema: PLUGIN_ID_SCHEMA },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "certificates"], PRIV_SYS_MODIFY, false),
    },
    protected: true,
)]
/// Delete an ACME plugin configuration.
pub fn delete_plugin(id: String) -> Result<(), Error> {
    proxmox_acme_api::delete_plugin(id)
}

#[api(
    access: {
        permission: &proxmox_router::Permission::Anybody,
    },
    returns: {
        description: "ACME Challenge Plugin Shema.",
        type: Array,
        items: { type: AcmeChallengeSchema },
    },
)]
/// Get named known ACME directory endpoints.
fn get_challenge_schema() -> Result<ChallengeSchemaWrapper, Error> {
    proxmox_acme_api::get_cached_challenge_schemas()
}

#[api(
    access: {
        permission: &proxmox_router::Permission::Anybody,
    },
    returns: {
        description: "List of known ACME directories.",
        type: Array,
        items: { type: KnownAcmeDirectory },
    },
)]
/// Get named known ACME directory endpoints.
fn get_directories() -> Result<&'static [KnownAcmeDirectory], Error> {
    Ok(proxmox_acme_api::KNOWN_ACME_DIRECTORIES)
}

#[api(
    input: {
        properties: {
            directory: {
                type: String,
                description: "The ACME Directory.",
                optional: true,
            },
        },
    },
    access: {
        permission: &proxmox_router::Permission::Anybody,
    },
    returns: {
        type: String,
        optional: true,
        description: "The ACME Directory's ToS URL, if any.",
    },
)]
/// Get the Terms of Service URL for an ACME directory.
async fn get_tos(directory: Option<String>) -> Result<Option<String>, Error> {
    proxmox_acme_api::get_tos(directory).await
}
