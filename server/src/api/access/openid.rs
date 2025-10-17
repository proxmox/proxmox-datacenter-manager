//! OpenID redirect/login API
use anyhow::{bail, format_err, Error};
use hyper::http::request::Parts;
use hyper::Response;
use serde_json::{json, Value};

use proxmox_access_control::types::{User, EMAIL_SCHEMA, FIRST_NAME_SCHEMA, LAST_NAME_SCHEMA};
use proxmox_access_control::CachedUserInfo;
use proxmox_auth_api::api::{assemble_csrf_prevention_token, ApiTicket, AuthContext};
use proxmox_auth_api::ticket::Ticket;
use proxmox_auth_api::types::{CreateTicketResponse, Userid};
use proxmox_lang::try_block;
use proxmox_openid::{OpenIdAuthenticator, OpenIdConfig};
use proxmox_router::{
    http_err, list_subdirs_api_method, ApiHandler, ApiMethod, ApiResponseFuture, Permission,
    Router, RpcEnvironment, SubdirMap,
};
use proxmox_schema::{api, ApiType, ObjectSchema, ParameterSchema, StringSchema};
use proxmox_sortable_macro::sortable;

use pdm_api_types::{
    OpenIdRealmConfig, HTTP_URL_SCHEMA, OPENID_DEFAULT_SCOPE_LIST, REALM_ID_SCHEMA,
};
use pdm_buildcfg::PDM_RUN_DIR_M;

use crate::auth;

fn openid_authenticator(
    realm_config: &OpenIdRealmConfig,
    redirect_url: &str,
) -> Result<OpenIdAuthenticator, Error> {
    let scopes: Vec<String> = realm_config
        .scopes
        .as_deref()
        .unwrap_or(OPENID_DEFAULT_SCOPE_LIST)
        .split(|c: char| c == ',' || c == ';' || char::is_ascii_whitespace(&c))
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    let mut acr_values = None;
    if let Some(ref list) = realm_config.acr_values {
        acr_values = Some(
            list.split(|c: char| c == ',' || c == ';' || char::is_ascii_whitespace(&c))
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect(),
        );
    }

    let config = OpenIdConfig {
        issuer_url: realm_config.issuer_url.clone(),
        client_id: realm_config.client_id.clone(),
        client_key: realm_config.client_key.clone(),
        prompt: realm_config.prompt.clone(),
        scopes: Some(scopes),
        acr_values,
    };
    OpenIdAuthenticator::discover(&config, redirect_url)
}

#[sortable]
pub const API_METHOD_OPENID_LOGIN: ApiMethod = ApiMethod::new_full(
    &ApiHandler::AsyncHttpBodyParameters(&create_ticket_http_only),
    ParameterSchema::Object(&ObjectSchema::new(
        "Get a new ticket as an HttpOnly cookie. Supports tickets via cookies.",
        &sorted!([
            ("state", false, &StringSchema::new("OpenId state.").schema()),
            (
                "code",
                false,
                &StringSchema::new("OpenId authorization code.").schema(),
            ),
            (
                "redirect-url",
                false,
                &StringSchema::new(
                    "Redirection Url. The client should set this to used server url.",
                )
                .schema(),
            ),
        ]),
    )),
)
.returns(::proxmox_schema::ReturnType::new(
    false,
    &CreateTicketResponse::API_SCHEMA,
))
.protected(true)
.access(None, &Permission::World)
.reload_timezone(true);

fn create_ticket_http_only(
    _parts: Parts,
    param: Value,
    _info: &ApiMethod,
    rpcenv: Box<dyn RpcEnvironment>,
) -> ApiResponseFuture {
    Box::pin(async move {
        use proxmox_rest_server::RestEnvironment;

        let code = param["code"]
            .as_str()
            .ok_or_else(|| format_err!("missing non-optional parameter: code"))?
            .to_owned();
        let state = param["state"]
            .as_str()
            .ok_or_else(|| format_err!("missing non-optional parameter: state"))?
            .to_owned();
        let redirect_url = param["redirect-url"]
            .as_str()
            .ok_or_else(|| format_err!("missing non-optional parameter: redirect-url"))?
            .to_owned();

        let env: &RestEnvironment = rpcenv
            .as_any()
            .downcast_ref::<RestEnvironment>()
            .ok_or_else(|| format_err!("detected wrong RpcEnvironment type"))?;

        let user_info = CachedUserInfo::new()?;
        let auth_context = auth::get_auth_context()
            .ok_or_else(|| format_err!("could not get authentication context"))?;

        let mut tested_username = None;

        let result = try_block!({
            let (realm, private_auth_state) =
                OpenIdAuthenticator::verify_public_auth_state(PDM_RUN_DIR_M!(), &state)?;

            let (domains, _digest) = pdm_config::domains::config()?;
            let config: OpenIdRealmConfig = domains.lookup("openid", &realm)?;
            let open_id = openid_authenticator(&config, &redirect_url)?;
            let info = open_id.verify_authorization_code_simple(&code, &private_auth_state)?;
            let name_attr = config.username_claim.as_deref().unwrap_or("sub");

            // Try to be compatible with previous versions
            let try_attr = match name_attr {
                "subject" => Some("sub"),
                "username" => Some("preferred_username"),
                _ => None,
            };

            let unique_name = if let Some(name) = info[name_attr]
                .as_str()
                .or_else(|| try_attr.and_then(|att| info[att].as_str()))
            {
                name.to_owned()
            } else {
                bail!("missing claim '{name_attr}'");
            };

            let user_id = Userid::try_from(format!("{unique_name}@{realm}"))?;
            tested_username = Some(unique_name);

            if !user_info.is_active_user_id(&user_id) {
                if config.autocreate.unwrap_or(false) {
                    let _lock = proxmox_access_control::user::lock_config()?;
                    let (mut user_config, _digest) = proxmox_access_control::user::config()?;

                    if let Ok(old_user) = user_config.lookup::<User>("user", user_id.as_str()) {
                        if let Some(false) = old_user.enable {
                            bail!("user '{user_id}' is disabled.");
                        } else {
                            bail!("autocreate user failed - '{user_id}' already exists.");
                        }
                    }

                    let firstname = info["given_name"]
                        .as_str()
                        .map(|n| n.to_string())
                        .filter(|n| FIRST_NAME_SCHEMA.parse_simple_value(n).is_ok());

                    let lastname = info["family_name"]
                        .as_str()
                        .map(|n| n.to_string())
                        .filter(|n| LAST_NAME_SCHEMA.parse_simple_value(n).is_ok());

                    let email = info["email"]
                        .as_str()
                        .map(|n| n.to_string())
                        .filter(|n| EMAIL_SCHEMA.parse_simple_value(n).is_ok());

                    let user = User {
                        userid: user_id.clone(),
                        comment: None,
                        enable: None,
                        expire: None,
                        firstname,
                        lastname,
                        email,
                    };

                    user_config.set_data(user.userid.as_str(), "user", &user)?;
                    proxmox_access_control::user::save_config(&user_config)?;
                } else {
                    bail!("user account '{user_id}' missing, disabled or expired.");
                }
            }

            let api_ticket = ApiTicket::Full(user_id.clone());
            let ticket = Ticket::new(auth_context.auth_prefix(), &api_ticket)?;
            let token = assemble_csrf_prevention_token(auth_context.csrf_secret(), &user_id);
            env.log_auth(user_id.as_str());

            Ok((user_id, ticket, token))
        });

        let (user_id, mut ticket, token) = result.map_err(|err| {
            let msg = err.to_string();
            env.log_failed_auth(tested_username, &msg);
            http_err!(UNAUTHORIZED, "{msg}")
        })?;

        let cookie = format!(
            "{}={}; Secure; SameSite=Lax; HttpOnly; Path=/;",
            auth_context.prefixed_auth_cookie_name(),
            ticket.sign(auth_context.keyring(), None)?,
        );

        let response = Response::builder()
            .header(hyper::http::header::CONTENT_TYPE, "application/json")
            .header(hyper::header::SET_COOKIE, cookie);

        let data = CreateTicketResponse {
            csrfprevention_token: Some(token),
            ticket: None,
            ticket_info: Some(ticket.ticket_info()),
            username: user_id,
        };

        Ok(response.body(
            json!({"data": data, "status": 200, "success": true })
                .to_string()
                .into(),
        )?)
    })
}

#[api(
    protected: true,
    input: {
        properties: {
            realm: {
                schema: REALM_ID_SCHEMA,
            },
            "redirect-url": {
                schema: HTTP_URL_SCHEMA,
            },
        },
    },
    returns: {
        description: "Redirection URL.",
        type: String,
    },
    access: {
        description: "Anyone can access this (before the user is authenticated).",
        permission: &Permission::World,
    },
)]
/// Create OpenID Redirect Session
pub fn openid_auth_url(
    realm: String,
    redirect_url: String,
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<String, Error> {
    let (domains, _digest) = pdm_config::domains::config()?;
    let config: OpenIdRealmConfig = domains.lookup("openid", &realm)?;

    let open_id = openid_authenticator(&config, &redirect_url)?;

    let url = open_id.authorize_url(PDM_RUN_DIR_M!(), &realm)?;

    Ok(url)
}

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("login", &Router::new().post(&API_METHOD_OPENID_LOGIN)),
    ("auth-url", &Router::new().post(&API_METHOD_OPENID_AUTH_URL)),
]);

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);
