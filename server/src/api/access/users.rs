//! User Management

use anyhow::{bail, format_err, Error};
use std::collections::HashMap;

use proxmox_access_control::types::{ApiToken, User, UserUpdater, UserWithTokens};
use proxmox_access_control::CachedUserInfo;
use proxmox_router::{ApiMethod, Permission, Router, RpcEnvironment, SubdirMap};
use proxmox_schema::api;

use pdm_api_types::{
    Authid, ConfigDigest, DeletableUserProperty, Userid, PDM_PASSWORD_SCHEMA, PRIV_ACCESS_MODIFY,
    PRIV_SYS_AUDIT,
};

fn new_user_with_tokens(user: User) -> UserWithTokens {
    UserWithTokens {
        user,
        tokens: Vec::new(),
        totp_locked: false,
        tfa_locked_until: None,
    }
}

#[api(
    input: {
        properties: {
            include_tokens: {
                type: bool,
                description: "Include user's API tokens in returned list.",
                optional: true,
                default: false,
            },
        },
    },
    returns: {
        description: "List users (with config digest).",
        type: Array,
        items: { type: UserWithTokens },
    },
    access: {
        permission: &Permission::Anybody,
        description: "Returns all or just the logged-in user (/API token owner), \
            depending on privileges.",
    },
)]
/// List users
pub fn list_users(
    include_tokens: bool,
    _info: &ApiMethod,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Vec<UserWithTokens>, Error> {
    let (config, digest) = proxmox_access_control::user::config()?;

    let auth_id: Authid = rpcenv
        .get_auth_id()
        .ok_or_else(|| format_err!("no authid available"))?
        .parse()?;

    let userid = auth_id.user();

    let user_info = CachedUserInfo::new()?;

    let top_level_privs = user_info.lookup_privs(&auth_id, &["access", "users"]);
    let top_level_allowed = (top_level_privs & PRIV_SYS_AUDIT) != 0;

    let filter_by_privs = |user: &User| top_level_allowed || user.userid == *userid;

    let list: Vec<User> = config.convert_to_typed_array("user")?;

    rpcenv["digest"] = hex::encode(digest).into();

    let iter = list.into_iter().filter(filter_by_privs);
    let list = if include_tokens {
        let tokens: Vec<ApiToken> = config.convert_to_typed_array("token")?;
        let mut user_to_tokens = tokens.into_iter().fold(
            HashMap::new(),
            |mut map: HashMap<Userid, Vec<ApiToken>>, token: ApiToken| {
                if token.tokenid.is_token() {
                    map.entry(token.tokenid.user().clone())
                        .or_default()
                        .push(token);
                }
                map
            },
        );
        iter.map(|user: User| {
            let mut user = new_user_with_tokens(user);
            user.tokens = user_to_tokens.remove(&user.user.userid).unwrap_or_default();
            user
        })
        .collect()
    } else {
        iter.map(new_user_with_tokens).collect()
    };

    Ok(list)
}

#[api(
    protected: true,
    input: {
        properties: {
            config: {
                type: User,
                flatten: true,
            },
            password: {
                schema: PDM_PASSWORD_SCHEMA,
                optional: true,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["access", "users"], PRIV_ACCESS_MODIFY, false),
    },
)]
/// Create new user.
pub fn create_user(
    password: Option<String>,
    config: User,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<(), Error> {
    let _lock = proxmox_access_control::user::lock_config()?;

    let (mut section_config, _digest) = proxmox_access_control::user::config()?;

    if section_config.sections.contains_key(config.userid.as_str()) {
        bail!("user '{}' already exists.", config.userid);
    }

    section_config.set_data(config.userid.as_str(), "user", &config)?;

    let realm = config.userid.realm();

    // Fails if realm does not exist!
    let authenticator = crate::auth::lookup_authenticator(realm)?;

    proxmox_access_control::user::save_config(&section_config)?;

    if let Some(password) = password {
        let user_info = CachedUserInfo::new()?;
        let current_auth_id: Authid = rpcenv.get_auth_id().unwrap().parse()?;
        if realm == "pam" && !user_info.is_superuser(&current_auth_id) {
            bail!("only superuser can edit pam credentials!");
        }
        let client_ip = rpcenv.get_client_ip().map(|sa| sa.ip());
        authenticator.store_password(config.userid.name(), &password, client_ip.as_ref())?;
    }

    Ok(())
}

#[api(
   input: {
        properties: {
            userid: {
                type: Userid,
            },
         },
    },
    returns: { type: User },
    access: {
        permission: &Permission::Or(&[
            &Permission::Privilege(&["access", "users"], PRIV_SYS_AUDIT, false),
            &Permission::UserParam("userid"),
        ]),
    },
)]
/// Read user configuration data.
pub fn read_user(userid: Userid, rpcenv: &mut dyn RpcEnvironment) -> Result<User, Error> {
    let (config, digest) = proxmox_access_control::user::config()?;
    let user = config.lookup("user", userid.as_str())?;
    rpcenv["digest"] = hex::encode(digest).into();
    Ok(user)
}

#[api(
    protected: true,
    input: {
        properties: {
            userid: {
                type: Userid,
            },
            update: {
                type: UserUpdater,
                flatten: true,
            },
            password: {
                schema: PDM_PASSWORD_SCHEMA,
                optional: true,
            },
            delete: {
                description: "List of properties to delete.",
                type: Array,
                optional: true,
                items: {
                    type: DeletableUserProperty,
                }
            },
            digest: {
                optional: true,
                type: ConfigDigest,
            },
        },
    },
    access: {
        permission: &Permission::Or(&[
            &Permission::Privilege(&["access", "users"], PRIV_ACCESS_MODIFY, false),
            &Permission::UserParam("userid"),
        ]),
    },
)]
/// Update user configuration.
#[allow(clippy::too_many_arguments)]
pub fn update_user(
    userid: Userid,
    update: UserUpdater,
    password: Option<String>,
    delete: Option<Vec<DeletableUserProperty>>,
    digest: Option<ConfigDigest>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<(), Error> {
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .ok_or_else(|| format_err!("no authid available"))?
        .parse()?;

    let _lock = proxmox_access_control::user::lock_config()?;

    let (mut config, config_digest) = proxmox_access_control::user::config()?;
    config_digest.detect_modification(digest.as_ref())?;

    let mut data: User = config.lookup("user", userid.as_str())?;

    let user_info = CachedUserInfo::new()?;
    let top_level_privs = user_info.lookup_privs(&auth_id, &["access", "users"]);
    let top_level_modify_allowed = (top_level_privs & PRIV_ACCESS_MODIFY) != 0;

    if let Some(delete) = delete {
        for delete_prop in delete {
            match delete_prop {
                DeletableUserProperty::Comment => data.comment = None,
                DeletableUserProperty::Firstname => data.firstname = None,
                DeletableUserProperty::Lastname => data.lastname = None,
                DeletableUserProperty::Email => data.email = None,
                DeletableUserProperty::Enable => data.enable = None,
                DeletableUserProperty::Expire => {
                    if !top_level_modify_allowed {
                        bail!("modifying expiration date not allowed");
                    }
                    data.expire = None;
                }
            }
        }
    }

    if let Some(comment) = update.comment {
        let comment = comment.trim().to_string();
        if comment.is_empty() {
            data.comment = None;
        } else {
            data.comment = Some(comment);
        }
    }

    if let Some(enable) = update.enable {
        data.enable = if enable { None } else { Some(false) };
    }

    if let Some(expire) = update.expire {
        if !top_level_modify_allowed {
            bail!("modifying expiration date not allowed");
        }
        data.expire = if expire > 0 { Some(expire) } else { None };
    }

    if let Some(password) = password {
        let current_auth_id: Authid = rpcenv.get_auth_id().unwrap().parse()?;
        let self_service = current_auth_id.user() == &userid;
        let target_realm = userid.realm();
        if !self_service && target_realm == "pam" && !user_info.is_superuser(&current_auth_id) {
            bail!("only superuser can edit pam credentials!");
        }
        let authenticator = crate::auth::lookup_authenticator(userid.realm())?;
        let client_ip = rpcenv.get_client_ip().map(|sa| sa.ip());
        authenticator.store_password(userid.name(), &password, client_ip.as_ref())?;
    }

    if let Some(firstname) = update.firstname {
        data.firstname = if firstname.is_empty() {
            None
        } else {
            Some(firstname)
        };
    }

    if let Some(lastname) = update.lastname {
        data.lastname = if lastname.is_empty() {
            None
        } else {
            Some(lastname)
        };
    }
    if let Some(email) = update.email {
        data.email = if email.is_empty() { None } else { Some(email) };
    }

    config.set_data(userid.as_str(), "user", &data)?;

    proxmox_access_control::user::save_config(&config)?;

    Ok(())
}

#[api(
    protected: true,
    input: {
        properties: {
            userid: { type: Userid },
            digest: {
                optional: true,
                type: ConfigDigest,
            },
        },
    },
    access: {
        permission: &Permission::Or(&[
            &Permission::Privilege(&["access", "users"], PRIV_ACCESS_MODIFY, false),
            &Permission::UserParam("userid"),
        ]),
    },
)]
/// Remove a user from the configuration file.
pub fn delete_user(userid: Userid, digest: Option<ConfigDigest>) -> Result<(), Error> {
    let _lock = proxmox_access_control::user::lock_config()?;
    let _tfa_lock = crate::auth::tfa::write_lock()?;

    let (mut config, config_digest) = proxmox_access_control::user::config()?;
    config_digest.detect_modification(digest.as_ref())?;

    match config.sections.get(userid.as_str()) {
        Some(_) => {
            config.sections.remove(userid.as_str());
        }
        None => bail!("user '{}' does not exist.", userid),
    }

    proxmox_access_control::user::save_config(&config)?;

    let authenticator = crate::auth::lookup_authenticator(userid.realm())?;
    match authenticator.remove_password(userid.name()) {
        Ok(()) => {}
        Err(err) => {
            eprintln!(
                "error removing password after deleting user {:?}: {}",
                userid, err
            );
        }
    }

    let update = || {
        let mut cfg = crate::auth::tfa::read()?;
        let _: proxmox_tfa::api::NeedsSaving =
            cfg.remove_user(&crate::auth::tfa::UserAccess, userid.as_str())?;
        crate::auth::tfa::write(&cfg)
    };
    match update() {
        Ok(()) => (),
        Err(err) => {
            eprintln!(
                "error updating TFA config after deleting user {:?}: {}",
                userid, err
            );
        }
    }

    Ok(())
}

const API_METHOD_READ_TOKEN_WITH_ACCESS: ApiMethod =
    proxmox_access_control::api::API_METHOD_READ_TOKEN.access(
        None,
        &Permission::Or(&[
            &Permission::Privilege(&["access", "users"], PRIV_SYS_AUDIT, false),
            &Permission::UserParam("userid"),
        ]),
    );

const API_METHOD_UPDATE_TOKEN_WITH_ACCESS: ApiMethod =
    proxmox_access_control::api::API_METHOD_UPDATE_TOKEN.access(
        None,
        &Permission::Or(&[
            &Permission::Privilege(&["access", "users"], PRIV_ACCESS_MODIFY, false),
            &Permission::UserParam("userid"),
        ]),
    );

const API_METHOD_GENERATE_TOKEN_WITH_ACCESS: ApiMethod =
    proxmox_access_control::api::API_METHOD_GENERATE_TOKEN.access(
        None,
        &Permission::Or(&[
            &Permission::Privilege(&["access", "users"], PRIV_ACCESS_MODIFY, false),
            &Permission::UserParam("userid"),
        ]),
    );

const API_METHOD_DELETE_TOKEN_WITH_ACCESS: ApiMethod =
    proxmox_access_control::api::API_METHOD_DELETE_TOKEN.access(
        None,
        &Permission::Or(&[
            &Permission::Privilege(&["access", "users"], PRIV_ACCESS_MODIFY, false),
            &Permission::UserParam("userid"),
        ]),
    );

const API_METHOD_LIST_TOKENS_WITH_ACCESS: ApiMethod =
    proxmox_access_control::api::API_METHOD_LIST_TOKENS.access(
        None,
        &Permission::Or(&[
            &Permission::Privilege(&["access", "users"], PRIV_SYS_AUDIT, false),
            &Permission::UserParam("userid"),
        ]),
    );

const TOKEN_ITEM_ROUTER: Router = Router::new()
    .get(&API_METHOD_READ_TOKEN_WITH_ACCESS)
    .put(&API_METHOD_UPDATE_TOKEN_WITH_ACCESS)
    .post(&API_METHOD_GENERATE_TOKEN_WITH_ACCESS)
    .delete(&API_METHOD_DELETE_TOKEN_WITH_ACCESS);

const TOKEN_ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_TOKENS_WITH_ACCESS)
    .match_all("token-name", &TOKEN_ITEM_ROUTER);

const USER_SUBDIRS: SubdirMap = &[("token", &TOKEN_ROUTER)];

const USER_ROUTER: Router = Router::new()
    .get(&API_METHOD_READ_USER)
    .put(&API_METHOD_UPDATE_USER)
    .delete(&API_METHOD_DELETE_USER)
    .subdirs(USER_SUBDIRS);

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_USERS)
    .post(&API_METHOD_CREATE_USER)
    .match_all("userid", &USER_ROUTER);
