//! Implements all the methods under `/api2/json/auto-install/`.

use anyhow::{anyhow, Context, Result};
use http::StatusCode;
use std::collections::{BTreeMap, HashMap};

use pdm_api_types::{
    auto_installer::{
        AnswerToken, AnswerTokenCreateResult, AnswerTokenUpdateResult, AnswerTokenUpdater,
        DeletableAnswerTokenProperty, DeletablePreparedInstallationConfigProperty, Installation,
        InstallationStatus, PreparedInstallationConfig, PreparedInstallationConfigCreateResult,
        PreparedInstallationConfigUpdateResult, PreparedInstallationConfigUpdater,
        INSTALLATION_UUID_SCHEMA, PREPARED_INSTALL_CONFIG_ID_SCHEMA, TEMPLATE_COUNTER_NAME_REGEX,
        UDEV_FILTER_KEY_REGEX,
    },
    Authid, ConfigDigest, PRIV_SYS_AUDIT, PRIV_SYS_MODIFY, PROXMOX_CONFIG_DIGEST_SCHEMA,
};
use pdm_config::auto_install::types::PreparedInstallationSectionConfigWrapper;
use proxmox_installer_types::{
    answer::{
        self, fetch::AnswerFetchData, AutoInstallerConfig, PostNotificationHookInfo,
        ROOT_PASSWORD_SCHEMA,
    },
    post_hook::PostHookInfo,
    SystemInfo,
};
use proxmox_network_types::fqdn::Fqdn;
use proxmox_router::{
    http_bail, list_subdirs_api_method, ApiHandler, ApiMethod, ApiResponseFuture, Permission,
    Router, RpcEnvironment, SubdirMap,
};
use proxmox_schema::{
    api, api_types::COMMENT_SCHEMA, AllOfSchema, ApiType, ParameterSchema, ReturnType,
};
use proxmox_sortable_macro::sortable;
use proxmox_uuid::Uuid;

#[sortable]
const SUBDIR_INSTALLATION_PER_ID: SubdirMap = &sorted!([(
    "post-hook",
    &Router::new().post(&API_METHOD_HANDLE_POST_HOOK)
)]);

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("answer", &Router::new().post(&API_METHOD_NEW_INSTALLATION)),
    (
        "installations",
        &Router::new().get(&API_METHOD_LIST_INSTALLATIONS).match_all(
            "uuid",
            &Router::new()
                .delete(&API_METHOD_DELETE_INSTALLATION)
                .subdirs(SUBDIR_INSTALLATION_PER_ID)
        )
    ),
    (
        "prepared",
        &Router::new()
            .get(&API_METHOD_LIST_PREPARED_ANSWERS)
            .post(&API_METHOD_CREATE_PREPARED_ANSWER)
            .match_all(
                "id",
                &Router::new()
                    .get(&API_METHOD_GET_PREPARED_ANSWER)
                    .put(&API_METHOD_UPDATE_PREPARED_ANSWER)
                    .delete(&API_METHOD_DELETE_PREPARED_ANSWER)
            )
    ),
    (
        "tokens",
        &Router::new()
            .get(&API_METHOD_LIST_TOKENS)
            .post(&API_METHOD_CREATE_TOKEN)
            .match_all(
                "id",
                &Router::new()
                    .put(&API_METHOD_UPDATE_TOKEN)
                    .delete(&API_METHOD_DELETE_TOKEN)
            )
    ),
]);

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

const API_METHOD_NEW_INSTALLATION: ApiMethod = ApiMethod::new_full(
    &ApiHandler::AsyncHttpBodyParameters(&api_function_new_installation),
    ParameterSchema::AllOf(&AllOfSchema::new(
        r#"\
    Handles the system information of a new machine to install.

    See also
    <https://pve.proxmox.com/wiki/Automated_Installation#Answer_Fetched_via_HTTP>"#,
        &[&<AnswerFetchData as ApiType>::API_SCHEMA],
    )),
)
.returns(ReturnType::new(
    false,
    &<AutoInstallerConfig as ApiType>::API_SCHEMA,
))
.access(
    Some("Implemented through specialized bearer tokens."),
    &Permission::World,
)
.protected(false);

/// Implements the "upper" API handling for /auto-install/answer, most importantly
/// the authentication through secret tokens.
fn api_function_new_installation(
    parts: http::request::Parts,
    param: serde_json::Value,
    _info: &ApiMethod,
    _rpcenv: Box<dyn RpcEnvironment>,
) -> ApiResponseFuture {
    Box::pin(async move {
        let auth_header = parts
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .unwrap_or_default();

        let token_id = match verify_answer_authorization_header(auth_header) {
            Some(token_id) => token_id,
            None => {
                return Ok(http::Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(String::new().into())?)
            }
        };

        let response = serde_json::from_value::<AnswerFetchData>(param)
            .map_err(|err| anyhow!("failed to deserialize body: {err:?}"))
            .and_then(|data| new_installation(&token_id, data))
            .map_err(|err| err.to_string())
            .and_then(|result| serde_json::to_string(&result).map_err(|err| err.to_string()));

        match response {
            Ok(body) => Ok(http::Response::builder()
                .status(StatusCode::OK)
                .header(
                    http::header::CONTENT_TYPE,
                    "application/json; charset=utf-8",
                )
                .body(body.into())?),
            Err(err) => Ok(http::Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(format!("{err:#}").into())?),
        }
    })
}

/// Verifies the given `Authorization` HTTP header value whether
/// a) It matches the required format, i.e. Bearer <token-id>:<secret>
/// b) The token secret is known and verifies successfully.
///
/// # Parameters
///
/// * `header` - The value of the `Authorization` header sent by the client
fn verify_answer_authorization_header(header: &str) -> Option<String> {
    let (scheme, token) = header.split_once(' ').unwrap_or_default();
    if scheme.to_lowercase() != "bearer" {
        return None;
    }

    let _lock = pdm_config::auto_install::tokens_read_lock();
    let (tokens, _) = pdm_config::auto_install::read_tokens().ok()?;

    let (id, secret) = token.split_once(':').unwrap_or_default();

    let token: AnswerToken = tokens.get(id)?.clone().into();
    if !token.is_active() {
        return None;
    }

    pdm_config::auto_install::verify_secret(id, secret).ok()?;

    Some(id.to_owned())
}

/// POST /auto-install/answer
///
/// Handles the system information of a new machine to install.
///
/// See also
/// <https://pve.proxmox.com/wiki/Automated_Installation#Answer_Fetched_via_HTTP>
///
/// Returns a auto-installer configuration if a matching one is found, otherwise errors out.
///
/// The system information data is saved in any case to make them easily inspectable.
fn new_installation(token_id: &String, payload: AnswerFetchData) -> Result<AutoInstallerConfig> {
    let _lock = pdm_config::auto_install::installations_write_lock();

    let uuid = Uuid::generate();
    let (mut installations, _) = pdm_config::auto_install::read_installations()?;

    if installations.iter().any(|p| p.uuid == uuid) {
        http_bail!(CONFLICT, "already exists");
    }

    let timestamp_now = proxmox_time::epoch_i64();

    if let Some(config) = find_config(token_id, &payload.sysinfo)? {
        let status = if config.post_hook_base_url.is_some() {
            InstallationStatus::InProgress
        } else {
            InstallationStatus::AnswerSent
        };

        let mut answer = render_prepared_config(&config, &payload.sysinfo)?;

        // Generate a per-installation secret for authenticating the post-hook
        // callback. The UUID alone is not a credential as it travels through
        // the answer payload back to the installer; the secret is short-lived
        // and cleared once the callback succeeded.
        let post_hook_token = config
            .post_hook_base_url
            .is_some()
            .then(|| hex::encode(proxmox_sys::linux::random_data(16).unwrap_or_default()));

        installations.push(Installation {
            uuid: uuid.clone(),
            received_at: timestamp_now,
            status,
            info: payload.sysinfo,
            answer_id: Some(config.id.clone()),
            post_hook_data: None,
            post_hook_token: post_hook_token.clone(),
        });

        // Inject our custom post hook if the user defined a base url
        if let Some(base_url) = config.post_hook_base_url {
            answer.post_installation_webhook = Some(PostNotificationHookInfo {
                url: format!(
                    "{}/api2/json/auto-install/installations/{uuid}/post-hook",
                    base_url.trim_end_matches('/')
                ),
                cert_fingerprint: config.post_hook_cert_fp.clone(),
                auth_token: post_hook_token,
            });
        }

        increment_template_counters(&config.id)?;
        pdm_config::auto_install::save_installations(&installations)?;
        Ok(answer)
    } else {
        installations.push(Installation {
            uuid: uuid.clone(),
            received_at: timestamp_now,
            status: InstallationStatus::NoAnswerFound,
            info: payload.sysinfo,
            answer_id: None,
            post_hook_data: None,
            post_hook_token: None,
        });

        pdm_config::auto_install::save_installations(&installations)?;
        http_bail!(NOT_FOUND, "no answer file found");
    }
}

#[api(
    returns: {
        description: "List of all automated installations.",
        type: Array,
        items: { type: Installation },
    },
    access: {
        permission: &Permission::Privilege(&["system", "auto-installation"], PRIV_SYS_AUDIT, false),
    },
)]
/// GET /auto-install/installations
///
/// Get all automated installations.
fn list_installations(rpcenv: &mut dyn RpcEnvironment) -> Result<Vec<Installation>> {
    let _lock = pdm_config::auto_install::installations_read_lock();

    let (mut config, digest) = pdm_config::auto_install::read_installations()?;

    // The post-hook secret is on-disk state only, never expose it to API clients.
    for inst in &mut config {
        inst.post_hook_token = None;
    }

    rpcenv["digest"] = hex::encode(digest).into();
    Ok(config)
}

#[api(
    input: {
        properties: {
            uuid: {
                schema: INSTALLATION_UUID_SCHEMA,
            }
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "auto-installation"], PRIV_SYS_MODIFY, false),
    },
)]
/// DELETE /auto-install/installations/{uuid}
///
/// Remove an installation entry.
fn delete_installation(uuid: Uuid) -> Result<()> {
    let _lock = pdm_config::auto_install::installations_write_lock();

    let (mut installations, _) = pdm_config::auto_install::read_installations()?;
    if installations
        .extract_if(.., |inst| inst.uuid == uuid)
        .count()
        == 0
    {
        http_bail!(NOT_FOUND, "no such entry {uuid:?}");
    }

    pdm_config::auto_install::save_installations(&installations)
}

#[api(
    returns: {
        description: "List of prepared auto-installer answer configurations.",
        type: Array,
        items: { type: PreparedInstallationConfig },
    },
    access: {
        permission: &Permission::Privilege(&["system", "auto-installation"], PRIV_SYS_AUDIT, false),
    },
)]
/// GET /auto-install/prepared
///
/// Get all prepared auto-installer answer configurations.
async fn list_prepared_answers(
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<PreparedInstallationConfig>> {
    let (prepared, digest) = pdm_config::auto_install::read_prepared_answers()?;

    rpcenv["digest"] = hex::encode(digest).into();

    prepared.values().try_fold(
        Vec::with_capacity(prepared.len()),
        |mut v, p| -> Result<Vec<PreparedInstallationConfig>, anyhow::Error> {
            let mut p: PreparedInstallationConfig = p.clone().try_into()?;
            p.root_password_hashed = None;
            v.push(p);
            Ok(v)
        },
    )
}

#[api(
    input: {
        properties: {
            config: {
                type: PreparedInstallationConfig,
                flatten: true,
            },
            "root-password": {
                schema: ROOT_PASSWORD_SCHEMA,
                optional: true,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "auto-installation"], PRIV_SYS_MODIFY, false),
    },
    returns: {
        type: PreparedInstallationConfigCreateResult,
    },
)]
/// POST /auto-install/prepared
///
/// Creates a new prepared answer file.
async fn create_prepared_answer(
    mut config: PreparedInstallationConfig,
    root_password: Option<String>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<PreparedInstallationConfigCreateResult> {
    let _lock = pdm_config::auto_install::prepared_answers_write_lock();
    let (mut prepared, _) = pdm_config::auto_install::read_prepared_answers()?;

    if prepared.contains_key(&config.id) {
        http_bail!(
            CONFLICT,
            "configuration with ID {} already exists",
            config.id
        );
    }

    if config.is_default {
        if let Some(PreparedInstallationSectionConfigWrapper::PreparedConfig(p)) = prepared
            .values()
            .find(|PreparedInstallationSectionConfigWrapper::PreparedConfig(p)| p.is_default)
        {
            http_bail!(
                CONFLICT,
                "configuration '{}' is already the default answer",
                p.id
            );
        }
    }

    if let Some(password) = root_password {
        config.root_password_hashed = Some(proxmox_sys::crypt::encrypt_pw(&password)?);
    } else if config.root_password_hashed.is_none() {
        http_bail!(
            BAD_REQUEST,
            "either `root-password` or `root-password-hashed` must be set"
        );
    }

    let token = if config.authorized_tokens.is_empty() {
        // if no token was specified, generate a new one
        let token = generate_token(&config.id, rpcenv)?;
        config.authorized_tokens.push(token.token.id.clone());
        Some(token)
    } else {
        None
    };

    validate_udev_filter_map(&config.netdev_filter)?;
    validate_udev_filter_map(&config.disk_filter)?;
    validate_template_map(&config.template_counters)?;

    prepared.insert(config.id.clone(), config.clone().try_into()?);
    pdm_config::auto_install::save_prepared_answers(&prepared)?;

    Ok(PreparedInstallationConfigCreateResult { config, token })
}

#[api(
    input: {
        properties: {
            id: {
                schema: PREPARED_INSTALL_CONFIG_ID_SCHEMA,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "auto-installation"], PRIV_SYS_AUDIT, false),
    },
)]
/// GET /auto-install/prepared/{id}
///
/// Retrieves a prepared auto-installer answer configuration.
async fn get_prepared_answer(id: String) -> Result<PreparedInstallationConfig> {
    let (prepared, _) = pdm_config::auto_install::read_prepared_answers()?;

    if let Some(PreparedInstallationSectionConfigWrapper::PreparedConfig(mut p)) =
        prepared.get(&id).cloned()
    {
        // Don't send the hashed password, the user cannot do anything with it anyway
        p.root_password_hashed = None;
        p.try_into()
    } else {
        http_bail!(NOT_FOUND, "no such prepared answer configuration: {id}");
    }
}

#[api(
    input: {
        properties: {
            id: {
                schema: PREPARED_INSTALL_CONFIG_ID_SCHEMA,
            },
            update: {
                type: PreparedInstallationConfigUpdater,
                flatten: true,
            },
            "root-password": {
                schema: ROOT_PASSWORD_SCHEMA,
                optional: true,
            },
            delete: {
                description: "List of properties to delete.",
                type: Array,
                optional: true,
                items: {
                    type: DeletablePreparedInstallationConfigProperty,
                }
            },
            digest: {
                optional: true,
                schema: PROXMOX_CONFIG_DIGEST_SCHEMA,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "auto-installation"], PRIV_SYS_MODIFY, false),
    },
    returns: {
        type: PreparedInstallationConfigCreateResult,
    },
)]
/// PUT /auto-install/prepared/{id}
///
/// Updates a prepared auto-installer answer configuration.
async fn update_prepared_answer(
    id: String,
    update: PreparedInstallationConfigUpdater,
    root_password: Option<String>,
    delete: Option<Vec<DeletablePreparedInstallationConfigProperty>>,
    digest: Option<ConfigDigest>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<PreparedInstallationConfigUpdateResult> {
    let _lock = pdm_config::auto_install::prepared_answers_write_lock();

    let (mut prepared, config_digest) = pdm_config::auto_install::read_prepared_answers()?;
    config_digest.detect_modification(digest.as_ref())?;

    if update.is_default.unwrap_or(false) {
        if let Some(PreparedInstallationSectionConfigWrapper::PreparedConfig(other)) =
            prepared.values().find(
                |PreparedInstallationSectionConfigWrapper::PreparedConfig(p)| {
                    p.is_default && p.id != id
                },
            )
        {
            http_bail!(
                CONFLICT,
                "configuration '{}' is already the default answer",
                other.id
            );
        }
    }

    let p = match prepared.get_mut(&id) {
        Some(PreparedInstallationSectionConfigWrapper::PreparedConfig(p)) => p,
        None => http_bail!(NOT_FOUND, "no such prepared answer configuration: {id}"),
    };

    if let Some(delete) = delete {
        for prop in delete {
            match prop {
                DeletablePreparedInstallationConfigProperty::TargetFilter => {
                    p.target_filter.clear();
                }
                DeletablePreparedInstallationConfigProperty::NetdevFilter => {
                    p.netdev_filter.clear();
                }
                DeletablePreparedInstallationConfigProperty::DiskFilter => {
                    p.disk_filter.clear();
                }
                DeletablePreparedInstallationConfigProperty::RootSshKeys => {
                    p.root_ssh_keys.clear();
                }
                DeletablePreparedInstallationConfigProperty::PostHookBaseUrl => {
                    p.post_hook_base_url = None;
                }
                DeletablePreparedInstallationConfigProperty::PostHookCertFp => {
                    p.post_hook_cert_fp = None;
                }
                DeletablePreparedInstallationConfigProperty::TemplateCounters => {
                    p.template_counters.clear();
                }
            }
        }
    }

    // Destructuring makes sure we don't forget any member
    let PreparedInstallationConfigUpdater {
        authorized_tokens,
        is_default,
        target_filter,
        country,
        fqdn,
        use_dhcp_fqdn,
        keyboard,
        mailto,
        timezone,
        root_password_hashed,
        reboot_on_error,
        reboot_mode,
        root_ssh_keys,
        use_dhcp_network,
        cidr,
        gateway,
        dns,
        netdev_filter,
        netif_name_pinning_enabled,
        filesystem,
        disk_mode,
        disk_list,
        disk_filter,
        disk_filter_match,
        post_hook_base_url,
        post_hook_cert_fp,
        template_counters,
    } = update;

    let mut new_token = None;
    if let Some(mut tokens) = authorized_tokens {
        if tokens.is_empty() {
            // if no token was specified, generate a new one
            let token = generate_token(&p.id, rpcenv)?;
            tokens.push(token.token.id.clone());
            new_token = Some(token);
        }

        p.authorized_tokens = tokens;
    }

    if let Some(is_default) = is_default {
        p.is_default = is_default;
    }

    if let Some(filter) = target_filter {
        **p.target_filter = filter;
    }

    if let Some(country) = country {
        p.country = country;
    }

    if let Some(fqdn) = fqdn {
        p.fqdn = fqdn;
    }

    if let Some(use_dhcp) = use_dhcp_fqdn {
        p.use_dhcp_fqdn = use_dhcp;
    }

    if let Some(keyboard) = keyboard {
        p.keyboard = keyboard;
    }

    if let Some(mailto) = mailto {
        p.mailto = mailto;
    }

    if let Some(timezone) = timezone {
        p.timezone = timezone;
    }

    if let Some(password) = root_password {
        p.root_password_hashed = Some(proxmox_sys::crypt::encrypt_pw(&password)?);
    } else if let Some(password) = root_password_hashed {
        p.root_password_hashed = Some(password);
    }

    if let Some(reboot_on_error) = reboot_on_error {
        p.reboot_on_error = reboot_on_error;
    }

    if let Some(reboot_mode) = reboot_mode {
        p.reboot_mode = reboot_mode;
    }

    if let Some(ssh_keys) = root_ssh_keys {
        p.root_ssh_keys = ssh_keys;
    }

    if let Some(use_dhcp) = use_dhcp_network {
        p.use_dhcp_network = use_dhcp;
    }

    if let Some(cidr) = cidr {
        p.cidr = Some(cidr);
    }

    if let Some(gateway) = gateway {
        p.gateway = Some(gateway);
    }

    if let Some(dns) = dns {
        p.dns = Some(dns);
    }

    if let Some(filter) = netdev_filter {
        validate_udev_filter_map(&filter)?;
        **p.netdev_filter = filter;
    }

    if let Some(enabled) = netif_name_pinning_enabled {
        p.netif_name_pinning_enabled = enabled;
    }

    if let Some(fs) = filesystem {
        *p.filesystem = fs;
    }

    if let Some(mode) = disk_mode {
        p.disk_mode = mode;
    }

    if let Some(list) = disk_list {
        p.disk_list = list;
    }

    if let Some(filter) = disk_filter {
        validate_udev_filter_map(&filter)?;
        **p.disk_filter = filter;
    }

    if let Some(filter_match) = disk_filter_match {
        p.disk_filter_match = Some(filter_match);
    }

    if let Some(url) = post_hook_base_url {
        p.post_hook_base_url = Some(url);
    }

    if let Some(fp) = post_hook_cert_fp {
        p.post_hook_cert_fp = Some(fp);
    }

    if let Some(counters) = template_counters {
        validate_template_map(&counters)?;
        **p.template_counters = counters;
    }

    let config = p.clone().try_into()?;
    pdm_config::auto_install::save_prepared_answers(&prepared)?;

    Ok(PreparedInstallationConfigCreateResult {
        config,
        token: new_token,
    })
}

#[api(
    input: {
        properties: {
            id: {
                schema: PREPARED_INSTALL_CONFIG_ID_SCHEMA,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "auto-installation"], PRIV_SYS_MODIFY, false),
    },
)]
/// DELETE /auto-install/prepared/{id}
///
/// Deletes a prepared auto-installer answer configuration.
async fn delete_prepared_answer(id: String) -> Result<()> {
    let _lock = pdm_config::auto_install::prepared_answers_write_lock();

    let (mut prepared, _) = pdm_config::auto_install::read_prepared_answers()?;
    if prepared.remove(&id).is_none() {
        http_bail!(NOT_FOUND, "no such entry '{id:?}'");
    }

    pdm_config::auto_install::save_prepared_answers(&prepared)
}

fn validate_udev_filter_map(map: &BTreeMap<String, String>) -> Result<()> {
    for k in map.keys() {
        if !UDEV_FILTER_KEY_REGEX.is_match(k) {
            http_bail!(BAD_REQUEST, "invalid udev filter key: '{k}'");
        }
    }
    Ok(())
}

fn validate_template_map<T>(map: &BTreeMap<String, T>) -> Result<()> {
    for k in map.keys() {
        if !TEMPLATE_COUNTER_NAME_REGEX.is_match(k) {
            http_bail!(BAD_REQUEST, "invalid template counter name: '{k}'");
        }
    }
    Ok(())
}

#[api(
    input: {
        properties: {
            uuid: {
                schema: INSTALLATION_UUID_SCHEMA,
            },
            token: {
                type: String,
                description: "Per-installation token issued together with the answer.",
            },
            info: {
                type: PostHookInfo,
                flatten: true,
            }
        },
    },
    access: {
        description: "Authenticated through the per-installation token issued \
                      together with the answer.",
        permission: &Permission::World,
    },
)]
/// POST /auto-install/installations/{uuid}/post-hook
///
/// Handles the post-installation hook for all installations.
async fn handle_post_hook(uuid: Uuid, token: String, info: PostHookInfo) -> Result<()> {
    let _lock = pdm_config::auto_install::installations_write_lock();
    let (mut installations, _) = pdm_config::auto_install::read_installations()?;

    let install = installations.iter_mut().find(|inst| {
        inst.uuid == uuid
            && inst.status == InstallationStatus::InProgress
            && inst.post_hook_data.is_none()
    });

    // Same generic error for unknown UUID, wrong status, already-handled hook,
    // or wrong token to avoid leaking which check failed.
    let not_found = || http_bail!(NOT_FOUND, "installation {uuid} not found");

    let install = match install {
        Some(install) => install,
        None => return not_found(),
    };

    let stored = install.post_hook_token.as_deref().unwrap_or_default();
    if stored.is_empty()
        || stored.len() != token.len()
        || !openssl::memcmp::eq(stored.as_bytes(), token.as_bytes())
    {
        return not_found();
    }

    install.status = InstallationStatus::Finished;
    install.post_hook_data = Some(info);
    install.post_hook_token = None;
    pdm_config::auto_install::save_installations(&installations)?;

    Ok(())
}

#[api(
    returns: {
        description: "List of tokens for authenticating automated installations requests.",
        type: Array,
        items: {
            type: AnswerToken,
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "auto-installation"], PRIV_SYS_AUDIT, false),
    },
)]
/// GET /auto-install/tokens
///
/// Get all tokens that can be used for authenticating automated installations requests.
async fn list_tokens(rpcenv: &mut dyn RpcEnvironment) -> Result<Vec<AnswerToken>> {
    let (tokens, digest) = pdm_config::auto_install::read_tokens()?;

    rpcenv["digest"] = hex::encode(digest).into();

    Ok(tokens.values().map(|t| t.clone().into()).collect())
}

#[api(
    input: {
        properties: {
            id: {
                type: String,
                description: "Token ID.",
            },
            comment: {
                schema: COMMENT_SCHEMA,
                optional: true,
            },
            enabled: {
                type: bool,
                description: "Whether the token is enabled.",
                default: true,
                optional: true,
            },
            "expire-at": {
                type: Integer,
                description: "Token expiration date, in seconds since the epoch. '0' means no expiration.",
                default: 0,
                minimum: 0,
                optional: true,
            },
        },
    },
    returns: {
        type: AnswerTokenCreateResult,
    },
    access: {
        permission: &Permission::Privilege(&["system", "auto-installation"], PRIV_SYS_MODIFY, false),
    },
    protected: true,
)]
/// POST /auto-install/tokens
///
/// Creates a new token for authenticating automated installations.
fn create_token(
    id: String,
    comment: Option<String>,
    enabled: Option<bool>,
    expire_at: Option<i64>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<AnswerTokenCreateResult> {
    let _lock = pdm_config::auto_install::tokens_write_lock();

    let authid = rpcenv
        .get_auth_id()
        .ok_or_else(|| anyhow!("no authid"))?
        .parse::<Authid>()?;

    let token = AnswerToken {
        id,
        created_by: authid.user().clone(),
        comment,
        enabled,
        expire_at,
    };
    let secret = Uuid::generate();

    pdm_config::auto_install::add_token(&token, &secret.to_string())
        .context("failed to create new token")?;

    Ok(AnswerTokenCreateResult {
        token,
        secret: secret.to_string(),
    })
}

#[api(
    input: {
        properties: {
            id: {
                type: String,
                description: "Token ID.",
            },
            update: {
                type: AnswerTokenUpdater,
                flatten: true,
            },
            delete: {
                type: Array,
                description: "List of properties to delete.",
                optional: true,
                items: {
                    type: DeletableAnswerTokenProperty,
                }
            },
            "regenerate-secret": {
                type: bool,
                description: "Whether to regenerate the current secret, invalidating the old one.",
                optional: true,
                default: false,
            },
            digest: {
                type: ConfigDigest,
                optional: true,
            },
        },
    },
    returns: {
        type: AnswerTokenUpdateResult,
    },
    access: {
        permission: &Permission::Privilege(&["system", "auto-installation"], PRIV_SYS_MODIFY, false),
    },
    protected: true,
)]
/// PUT /auto-install/tokens/{id}
///
/// Updates an existing access token.
async fn update_token(
    id: String,
    update: AnswerTokenUpdater,
    delete: Option<Vec<DeletableAnswerTokenProperty>>,
    regenerate_secret: bool,
    digest: Option<ConfigDigest>,
) -> Result<AnswerTokenUpdateResult> {
    let _lock = pdm_config::auto_install::tokens_write_lock();
    let (tokens, config_digest) = pdm_config::auto_install::read_tokens()?;

    config_digest.detect_modification(digest.as_ref())?;

    let mut token: AnswerToken = match tokens.get(&id.to_string()).cloned() {
        Some(secret) => secret.into(),
        None => http_bail!(NOT_FOUND, "no such access token: {id}"),
    };

    if let Some(delete) = delete {
        for prop in delete {
            match prop {
                DeletableAnswerTokenProperty::Comment => token.comment = None,
                DeletableAnswerTokenProperty::ExpireAt => token.expire_at = None,
            }
        }
    }

    let AnswerTokenUpdater {
        comment,
        enabled,
        expire_at,
    } = update;

    if let Some(comment) = comment {
        token.comment = Some(comment);
    }

    if let Some(enabled) = enabled {
        token.enabled = Some(enabled);
    }

    if let Some(expire_at) = expire_at {
        token.expire_at = Some(expire_at);
    }

    if regenerate_secret {
        // If the user instructed to update secret, just delete + re-create the token and let
        // the config implementation handle updating the shadow
        pdm_config::auto_install::delete_token(&token.id)?;

        let secret = Uuid::generate().to_string();
        pdm_config::auto_install::add_token(&token, &secret)?;

        Ok(AnswerTokenUpdateResult {
            token,
            secret: Some(secret),
        })
    } else {
        pdm_config::auto_install::update_token(&token).context("failed to update token")?;

        Ok(AnswerTokenUpdateResult {
            token,
            secret: None,
        })
    }
}

#[api(
    input: {
        properties: {
            id: {
                type: String,
                description: "Token ID.",
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "auto-installation"], PRIV_SYS_MODIFY, false),
    },
    protected: true,
)]
/// DELETE /auto-install/tokens/{id}
///
/// Deletes a prepared auto-installer answer configuration.
///
/// If the token is currently in use by any prepared answer configuration, the deletion will fail.
async fn delete_token(id: String) -> Result<()> {
    // first check if the token is used anywhere
    let (prepared, _) = pdm_config::auto_install::read_prepared_answers()?;

    let used = prepared
        .values()
        .filter_map(|p| {
            let PreparedInstallationSectionConfigWrapper::PreparedConfig(p) = p;
            p.authorized_tokens.contains(&id).then(|| p.id.clone())
        })
        .collect::<Vec<String>>();

    if !used.is_empty() {
        http_bail!(
            CONFLICT,
            "token still in use by answer configurations: {}",
            used.join(", ")
        );
    }

    let _lock = pdm_config::auto_install::tokens_write_lock();
    pdm_config::auto_install::delete_token(&id)
}

/// Tries to find a prepared answer configuration matching the given target node system
/// information.
///
/// # Parameters
///
/// * `token_id` - ID of the authorization token.
/// * `info` - System information of the machine to be installed.
///
/// # Returns
///
/// * `Ok(Some(answer))` if a matching answer was found, containing the most specified answer that
///   matched.
/// * `Ok(None)` if no answer was matched and no default one exists, either.
/// * `Err(..)` if some error occurred.
fn find_config(
    token_id: &String,
    info: &proxmox_installer_types::SystemInfo,
) -> Result<Option<PreparedInstallationConfig>> {
    let info = serde_json::to_value(info)?;
    let (prepared, _) = pdm_config::auto_install::read_prepared_answers()?;

    let mut default_answer = None;
    for sc in prepared.values() {
        let PreparedInstallationSectionConfigWrapper::PreparedConfig(p) = sc;

        if !p.authorized_tokens.contains(token_id) {
            continue;
        }

        if p.is_default {
            // Save the default answer for later and use it if no other matched before that
            default_answer = Some(p.clone());
            continue;
        }

        if p.target_filter.is_empty() {
            // Not default answer and empty target filter, can never match
            continue;
        }

        let matched_all = p.target_filter.iter().all(|filter| {
            // Retrieve the value the key (aka. a JSON pointer) points to
            if let Some(value) = info.pointer(filter.0).and_then(|v| v.as_str()) {
                // .. and match it against the given value glob
                match glob::Pattern::new(filter.1) {
                    Ok(pattern) => pattern.matches(value),
                    _ => false,
                }
            } else {
                false
            }
        });

        if matched_all {
            return Ok(Some(p.clone().try_into()?));
        }
    }

    // If no specific target filter(s) matched, return the default answer, if there is one
    default_answer.map(|a| a.try_into()).transpose()
}

/// Renders a given [`PreparedInstallationConfig`] into the target [`AutoInstallerConfig`] struct.
///
/// Converts all types as needed and renders out Handlebar templates in applicable fields.
/// Currently, templating is supported for the following fields:
///
/// * `fqdn`
/// * `mailto`
/// * `cidr`
/// * `dns`
/// * `gateway`
fn render_prepared_config(
    conf: &PreparedInstallationConfig,
    sysinfo: &SystemInfo,
) -> Result<AutoInstallerConfig> {
    use pdm_api_types::auto_installer::DiskSelectionMode;
    use proxmox_installer_types::answer::{Filesystem, FilesystemOptions};

    let jinja = minijinja::Environment::new();
    let mut context = serde_json::to_value(sysinfo)?;
    if let Some(obj) = context.as_object_mut() {
        for (k, v) in conf.template_counters.iter() {
            obj.insert(k.clone(), (*v).into());
        }
    }

    let fqdn = if conf.use_dhcp_fqdn {
        answer::FqdnConfig::from_dhcp(None)
    } else {
        let fqdn = jinja.render_named_str("fqdn", &conf.fqdn.to_string(), &context)?;
        answer::FqdnConfig::Simple(Fqdn::from(&fqdn)?)
    };

    let mailto = jinja.render_named_str("mailto", &conf.mailto, &context)?;

    let global = answer::GlobalOptions {
        country: conf.country.clone(),
        fqdn,
        keyboard: conf.keyboard,
        mailto,
        timezone: conf.timezone.clone(),
        root_password: None,
        root_password_hashed: conf.root_password_hashed.clone(),
        reboot_on_error: conf.reboot_on_error,
        reboot_mode: conf.reboot_mode,
        root_ssh_keys: conf.root_ssh_keys.clone(),
    };

    let network = {
        let interface_name_pinning = conf.netif_name_pinning_enabled.then_some(
            answer::NetworkInterfacePinningOptionsAnswer {
                enabled: true,
                mapping: HashMap::new(),
            },
        );

        if conf.use_dhcp_network {
            answer::NetworkConfig::FromDhcp(answer::NetworkConfigFromDhcp {
                interface_name_pinning,
            })
        } else {
            let cidr = conf
                .cidr
                .ok_or_else(|| anyhow!("no host address"))
                .and_then(|cidr| Ok(jinja.render_named_str("cidr", &cidr.to_string(), &context)?))
                .and_then(|s| Ok(s.parse()?))?;

            let dns = conf
                .dns
                .ok_or_else(|| anyhow!("no DNS server address"))
                .and_then(|cidr| Ok(jinja.render_named_str("dns", &cidr.to_string(), &context)?))
                .and_then(|s| Ok(s.parse()?))?;

            let gateway = conf
                .gateway
                .ok_or_else(|| anyhow!("no gateway address"))
                .and_then(|cidr| {
                    Ok(jinja.render_named_str("gateway", &cidr.to_string(), &context)?)
                })
                .and_then(|s| Ok(s.parse()?))?;

            answer::NetworkConfig::FromAnswer(answer::NetworkConfigFromAnswer {
                cidr,
                dns,
                gateway,
                filter: conf.netdev_filter.clone(),
                interface_name_pinning,
            })
        }
    };

    let (disk_list, filter) = if conf.disk_mode == DiskSelectionMode::Fixed {
        (conf.disk_list.clone(), BTreeMap::new())
    } else {
        (vec![], conf.disk_filter.clone())
    };

    let disks = answer::DiskSetup {
        filesystem: match conf.filesystem {
            FilesystemOptions::Ext4(_) => Filesystem::Ext4,
            FilesystemOptions::Xfs(_) => Filesystem::Xfs,
            FilesystemOptions::Zfs(_) => Filesystem::Zfs,
            FilesystemOptions::Btrfs(_) => Filesystem::Btrfs,
        },
        disk_list,
        filter,
        filter_match: conf.disk_filter_match,
        zfs: match conf.filesystem {
            FilesystemOptions::Zfs(opts) => Some(opts),
            _ => None,
        },
        lvm: match conf.filesystem {
            FilesystemOptions::Ext4(opts) | FilesystemOptions::Xfs(opts) => Some(opts),
            _ => None,
        },
        btrfs: match conf.filesystem {
            FilesystemOptions::Btrfs(opts) => Some(opts),
            _ => None,
        },
    };

    Ok(AutoInstallerConfig {
        global,
        network,
        disks,
        post_installation_webhook: None,
        first_boot: None,
    })
}

/// Increments all counters of a given template by one.
///
/// # Parameters
///
/// `id` - ID of the template to update.
fn increment_template_counters(id: &str) -> Result<()> {
    let _lock = pdm_config::auto_install::prepared_answers_write_lock();
    let (mut prepared, _) = pdm_config::auto_install::read_prepared_answers()?;

    let conf = match prepared.get_mut(id) {
        Some(PreparedInstallationSectionConfigWrapper::PreparedConfig(p)) => p,
        None => http_bail!(NOT_FOUND, "no such prepared answer configuration: {id}"),
    };

    conf.template_counters
        .values_mut()
        .for_each(|v| *v = v.saturating_add(1));

    pdm_config::auto_install::save_prepared_answers(&prepared)?;
    Ok(())
}

fn generate_token(
    config_id: &str,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<AnswerTokenCreateResult> {
    let id = format!(
        "{}-{}",
        config_id,
        hex::encode(proxmox_sys::linux::random_data(4)?)
    );

    create_token(
        id.clone(),
        Some("Automatically generated.".to_owned()),
        Some(true),
        None,
        rpcenv,
    )
}
