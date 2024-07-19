use anyhow::{bail, format_err, Context as _, Error};
use serde_json::Value;

use proxmox_access_control::types::User;
use proxmox_fido2::FidoOpt;
use proxmox_router::cli::{
    format_and_print_result, get_output_format, CliCommand, CliCommandMap, CommandLineInterface,
    OUTPUT_FORMAT,
};
use proxmox_schema::api;
use proxmox_tfa::TfaType;

use pdm_api_types::{DeletableUserProperty, Userid};

use crate::{client, env};

pub fn cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert("list", CliCommand::new(&API_METHOD_LIST_USERS))
        .insert(
            "create",
            CliCommand::new(&API_METHOD_CREATE_USER).arg_param(&["userid"]),
        )
        .insert(
            "update",
            CliCommand::new(&API_METHOD_UPDATE_USER).arg_param(&["userid"]),
        )
        .insert(
            "passwd",
            CliCommand::new(&API_METHOD_CHANGE_USER_PASSWORD).arg_param(&["userid"]),
        )
        .insert(
            "delete",
            CliCommand::new(&API_METHOD_DELETE_USER).arg_param(&["userid"]),
        )
        .insert("tfa", tfa_cli())
        .into()
}

fn tfa_cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert("list", CliCommand::new(&API_METHOD_LIST_USER_TFA))
        .insert(
            "add",
            CliCommand::new(&API_METHOD_ADD_TFA).arg_param(&["type", "description"]),
        )
        .insert(
            "remove",
            CliCommand::new(&API_METHOD_REMOVE_TFA).arg_param(&["id"]),
        )
        //.insert(
        //    "update",
        //    CliCommand::new(&API_METHOD_UPDATE_TFA).arg_param(&["id"]),
        //)
        .into()
}

#[api(
    input: {
        properties: {
            "output-format": {
                schema: OUTPUT_FORMAT,
                optional: true,
            },
        }
    }
)]
/// List all users or show a single user's information.
async fn list_users(param: Value) -> Result<(), Error> {
    let output_format = get_output_format(&param);

    let client = client()?;

    let entries = client.list_users(false).await?;

    if output_format == "text" {
        if entries.is_empty() {
            println!("No users configured");
            return Ok(());
        }

        for entry in entries {
            let enabled = if entry.user.enable.unwrap_or(true) {
                "✓"
            } else {
                "✗"
            };

            println!("{enabled} {}", entry.user.userid);
            if let Some(value) = &entry.user.email {
                println!("  email: {value}");
            }
            if let Some(value) = &entry.user.firstname {
                println!("  first name: {value}");
            }
            if let Some(value) = &entry.user.lastname {
                println!("  last name: {value}");
            }
            if let Some(value) = &entry.user.comment {
                println!("  comment: {value}");
            }
            if let Some(value) = entry.user.expire {
                println!("  expires: {}", crate::time::format_epoch_lossy(value));
            }
        }
    } else {
        let data = serde_json::to_value(entries)?;
        format_and_print_result(&data, &output_format);
    }
    Ok(())
}

#[api(
    input: {
        properties: {
            user: {
                type: User,
                flatten: true,
            },
            password: {
                schema: proxmox_schema::api_types::PASSWORD_SCHEMA,
                optional: true,
            },
        }
    }
)]
/// List all users or show a single user's information.
async fn create_user(user: User, password: Option<String>) -> Result<(), Error> {
    let client = client()?;

    let password = if password.is_some() {
        password
    } else {
        let password = proxmox_sys::linux::tty::read_password("New password: ")?;
        if password.is_empty() {
            None
        } else {
            Some(
                String::from_utf8(password)
                    .map_err(|_| format_err!("password must be valid utf-8"))?,
            )
        }
    };

    client.create_user(&user, password.as_deref()).await?;
    Ok(())
}

#[api(
    input: {
        properties: {
            userid: { type: Userid },
        }
    }
)]
/// List all users or show a single user's information.
async fn delete_user(userid: Userid) -> Result<(), Error> {
    client()?.delete_user(userid.as_str()).await?;
    Ok(())
}

#[api(
    input: {
        properties: {
            userid: { type: Userid },
            user: {
                type: proxmox_access_control::types::UserUpdater,
                flatten: true,
            },
            delete: {
                description: "Clear/reset user properties.",
                optional: true,
                type: Array,
                items: {
                    type: DeletableUserProperty,
                },
            },
        }
    }
)]
/// Change user information.
async fn update_user(
    userid: Userid,
    user: proxmox_access_control::types::UserUpdater,
    delete: Option<Vec<DeletableUserProperty>>,
) -> Result<(), Error> {
    let client = client()?;

    client
        .update_user(
            userid.as_str(),
            &user,
            None,
            delete.as_deref().unwrap_or_default(),
        )
        .await?;
    Ok(())
}

#[api(
    input: {
        properties: {
            userid: { type: Userid },
            password: {
                schema: proxmox_schema::api_types::PASSWORD_SCHEMA,
                optional: true,
            },
        }
    }
)]
/// Change a user's password. If no password is provided, it will be prompted for interactively.
async fn change_user_password(userid: Userid, password: Option<String>) -> Result<(), Error> {
    let client = client()?;

    let password = if password.is_some() {
        password
    } else {
        let password = proxmox_sys::linux::tty::read_password("New password: ")?;
        if password.is_empty() {
            None
        } else {
            Some(
                String::from_utf8(password)
                    .map_err(|_| format_err!("password must be valid utf-8"))?,
            )
        }
    };

    client
        .update_user(
            userid.as_str(),
            &Default::default(),
            password.as_deref(),
            &[],
        )
        .await?;
    Ok(())
}

#[api(
    input: {
        properties: {
            "output-format": {
                schema: OUTPUT_FORMAT,
                optional: true,
            },
            userid: { optional: true },
        }
    }
)]
/// List all the remotes this instance is managing.
async fn list_user_tfa(userid: Option<Userid>, param: Value) -> Result<(), Error> {
    let userid = userid
        .or_else(|| env().connect_args.user.clone())
        .ok_or_else(|| format_err!("missing userid and no user logged in?"))?;

    let output_format = get_output_format(&param);

    let entries = client()?.list_user_tfa(userid.as_str()).await?;

    if output_format == "text" {
        if entries.is_empty() {
            println!("No TFA entries configured");
            return Ok(());
        }

        for entry in entries {
            let enabled = if entry.info.enable { "✓" } else { "✗" };

            println!("{enabled} {}: {}", entry.ty, entry.info.id);
            // FIXME: print a nicer date...
            println!("    created: {}", entry.info.created);
            if !entry.info.description.is_empty() {
                println!("    {}", entry.info.description);
            }
        }
    } else {
        let data = serde_json::to_value(entries)?;
        format_and_print_result(&data, &output_format);
    }
    Ok(())
}

#[api(
    input: {
        properties: {
            userid: {
                description: "userid",
                optional: true,
            },
            "type": { type: TfaType },
            description: { description: "a description for the tfa entry" },
        }
    }
)]
/// Add a TFA method to a user (currently only recovery keys).
async fn add_tfa(
    userid: Option<String>,
    r#type: TfaType,
    description: String,
) -> Result<(), Error> {
    let env_userid = env().need_userid()?;

    let userid = userid
        .or_else(|| Some(env_userid.to_string()))
        .ok_or_else(|| format_err!("missing userid and no user logged in?"))?;

    let password = if env_userid != "root@pam" && userid != env_userid.as_str() {
        let password = proxmox_sys::linux::tty::read_password("Password: ")?;
        Some(String::from_utf8(password)?)
    } else {
        None
    };

    match r#type {
        TfaType::Recovery => add_recovery(userid, password, description).await,
        TfaType::Webauthn => add_webauthn(userid, password, description).await,
        other => bail!("adding tfa entries of type {other} is currently not supported"),
    }
}

async fn add_recovery(
    userid: String,
    password: Option<String>,
    description: String,
) -> Result<(), Error> {
    let keys = client()?
        .add_recovery_keys(&userid, password.as_deref(), &description)
        .await?;

    for (n, key) in keys.into_iter().enumerate() {
        println!("{n}: {key}");
    }

    Ok(())
}

async fn add_webauthn(
    userid: String,
    password: Option<String>,
    description: String,
) -> Result<(), Error> {
    let uri = env()
        .url()?
        .parse()
        .context("internal error: failed to parse generated URL")?;

    let client = client()?;
    let challenge_str = client
        .add_webauthn(&userid, password.as_deref(), &description)
        .await?;

    let challenge = serde_json::from_str(&challenge_str)
        .context("failed to decode webauthn credential creation challenge")?;

    let response = perform_fido_creation(&uri, &challenge)?;

    let id = client
        .add_webauthn_finish(
            &userid,
            password.as_deref(),
            &challenge.public_key.challenge.to_string(),
            &response,
        )
        .await?;

    println!("created entry with id {id:?}");

    Ok(())
}

#[api(
    input: {
        properties: {
            userid: {
                description: "userid",
                optional: true,
            },
            id: { description: "the tfa id to remove" },
        }
    }
)]
/// Remove a TFA entry by id.
async fn remove_tfa(userid: Option<String>, id: String) -> Result<(), Error> {
    let env_userid = env().need_userid()?;

    let userid = userid
        .or_else(|| Some(env_userid.to_string()))
        .ok_or_else(|| format_err!("missing userid and no user logged in?"))?;

    let password = if userid != env_userid.as_str() {
        let password = proxmox_sys::linux::tty::read_password("Password: ")?;
        Some(String::from_utf8(password)?)
    } else {
        None
    };

    Ok(client()?
        .remove_tfa_entry(&userid, password.as_deref(), &id)
        .await?)
}

fn perform_fido_creation(
    api_url: &http::Uri,
    challenge: &webauthn_rs::proto::CreationChallengeResponse,
) -> Result<String, Error> {
    let public_key = &challenge.public_key;
    let raw_challenge: &[u8] = public_key.challenge.as_ref();
    let b64u_challenge = base64::encode_config(raw_challenge, base64::URL_SAFE_NO_PAD);
    let client_data_json = serde_json::to_string(&serde_json::json!({
        "type": "webauthn.create",
        "origin": api_url.to_string().trim_end_matches('/'),
        "challenge": b64u_challenge.as_str(),
    }))
    .expect("failed to build json string");
    let hash = openssl::sha::sha256(client_data_json.as_bytes());

    let libfido = proxmox_fido2::Lib::open()?;

    'device: for dev_info in libfido.list_devices(None)? {
        log::debug!(
            "opening FIDO2 device {manufacturer:?} {product:?} at {path:?}",
            manufacturer = dev_info.manufacturer,
            product = dev_info.product,
            path = dev_info.path,
        );
        let dev = match libfido.dev_open(&dev_info.path) {
            Ok(dev) => dev,
            Err(err) => {
                log::debug!(
                    "failed to open FIDO2 device {path:?} - {err}",
                    path = dev_info.path,
                );
                continue;
            }
        };
        let options = match dev.options() {
            Ok(o) => o,
            Err(err) => {
                log::error!(
                    "error getting device options for {path:?}: {err:?}",
                    path = dev_info.path
                );
                continue 'device;
            }
        };

        'algorithm: for params in &public_key.pub_key_cred_params {
            let Ok(alg) = libc::c_int::try_from(params.alg) else {
                continue 'algorithm;
            };

            let Some(cred) = prepare_cerds(&libfido, public_key, &hash, &options, alg)? else {
                continue 'device;
            };

            let mut pin = None;
            'with_pin: loop {
                match dev.make_cred(&cred, pin.as_deref()) {
                    Ok(()) => return finish_fido_auth(cred, client_data_json, b64u_challenge, alg),
                    Err(proxmox_fido2::Error::UnsupportedAlgorithm) => continue 'algorithm,
                    Err(proxmox_fido2::Error::PinRequired) if pin.is_none() => {
                        let user_pin = proxmox_sys::linux::tty::read_password("fido2 pin: ")?;
                        pin = Some(
                            String::from_utf8(user_pin)
                                .map_err(|_| format_err!("invalid bytes in pin"))?,
                        );
                        continue 'with_pin;
                    }
                    Err(err) => return Err(err.into()),
                }
            }
        }

        // this device supports none of the algorithms, try another
    }

    bail!("failed to perform fido2 authentication");
}

fn prepare_cerds(
    libfido: &std::sync::Arc<proxmox_fido2::Lib>,
    public_key: &webauthn_rs::proto::PublicKeyCredentialCreationOptions,
    clientdata_hash: &[u8; 32],
    options: &proxmox_fido2::DeviceOptions,
    alg: i32,
) -> Result<Option<proxmox_fido2::FidoCred>, Error> {
    use webauthn_rs::proto::UserVerificationPolicy;

    let mut cred = libfido
        .cred_new()?
        .set_relying_party(public_key.rp.id.as_str(), public_key.rp.name.as_str())?
        .set_userid(
            public_key.user.id.as_ref(),
            Some(public_key.user.name.as_str()),
            Some(public_key.user.display_name.as_str()),
            None,
        )?
        .set_clientdata_hash(clientdata_hash)?
        .set_cose_type(alg)?;

    for excluded_cred in public_key.exclude_credentials.iter().flatten() {
        let excluded_cred = serde_json::to_value(excluded_cred)
            .context("failed to jsonify webauthn data")?
            .as_object_mut()
            .and_then(|obj| obj.remove("id"))
            .ok_or_else(|| {
                format_err!(
                    "webauthn creation challenge misses 'id' property in excluded credentials"
                )
            })?;
        let excluded_cred: webauthn_rs::base64_data::Base64UrlSafeData =
            serde_json::from_value(excluded_cred)
                .context("failed to extract excluded credential id in creation challenge")?;
        cred = cred.exclude_cred(excluded_cred.as_ref())?;
    }

    if let Some(criteria) = &public_key.authenticator_selection {
        cred = cred
            .set_resident_key(if criteria.require_resident_key {
                if !options.resident_key {
                    return Ok(None);
                }
                FidoOpt::True
            } else {
                FidoOpt::False
            })?
            .set_user_verification(match criteria.user_verification {
                UserVerificationPolicy::Discouraged => {
                    if options.user_verification {
                        FidoOpt::False
                    } else {
                        FidoOpt::Omit
                    }
                }
                UserVerificationPolicy::Preferred_DO_NOT_USE => FidoOpt::Omit,
                UserVerificationPolicy::Required => {
                    if !options.user_verification {
                        return Ok(None);
                    }
                    FidoOpt::True
                }
            })?;
    } else {
        cred = cred
            .set_resident_key(FidoOpt::Omit)?
            .set_user_verification(FidoOpt::Omit)?;
    }

    Ok(Some(cred))
}

fn finish_fido_auth(
    cred: proxmox_fido2::FidoCred,
    client_data_json: String,
    b64u_challenge: String,
    alg: i32,
) -> Result<String, Error> {
    use webauthn_rs::base64_data::Base64UrlSafeData;

    let id = cred.id()?;
    let sig = cred.signature()?;
    let x5c = cred.x5c()?;
    let auth_data = cred.auth_data()?;
    let auth_data = match serde_cbor::from_slice::<serde_cbor::Value>(auth_data)? {
        serde_cbor::Value::Bytes(bytes) => bytes,
        _ => bail!("auth data has invalid format"),
    };

    let mut stmt = std::collections::BTreeMap::new();
    stmt.insert(
        serde_cbor::Value::Text("alg".to_string()),
        serde_cbor::Value::from(alg),
    );
    stmt.insert(
        serde_cbor::Value::Text("sig".to_string()),
        serde_cbor::Value::from(sig.to_vec()),
    );
    stmt.insert(
        serde_cbor::Value::Text("x5c".to_string()),
        serde_cbor::Value::Array(vec![serde_cbor::Value::from(x5c.to_vec())]),
    );
    let mut obj = std::collections::BTreeMap::new();
    obj.insert(
        serde_cbor::Value::Text("fmt".to_string()),
        serde_cbor::Value::Text("packed".to_string()),
    );
    obj.insert(
        serde_cbor::Value::Text("attStmt".to_string()),
        serde_cbor::Value::Map(stmt),
    );
    obj.insert(
        serde_cbor::Value::Text("authData".to_string()),
        serde_cbor::Value::from(auth_data),
    );
    let attestation_object =
        serde_cbor::to_vec(&obj).context("failed to CBOR-encode attestation object")?;

    let response = webauthn_rs::proto::RegisterPublicKeyCredential {
        type_: "public-key".to_string(),
        id: base64::encode_config(id, base64::URL_SAFE_NO_PAD),
        raw_id: Base64UrlSafeData(id.to_vec()),
        response: webauthn_rs::proto::AuthenticatorAttestationResponseRaw {
            attestation_object: Base64UrlSafeData(attestation_object),
            client_data_json: Base64UrlSafeData(client_data_json.into_bytes()),
        },
    };

    let mut response = serde_json::to_value(response)?;
    response["response"]
        .as_object_mut()
        .unwrap()
        .remove("userHandle");
    response.as_object_mut().unwrap().remove("extensions");
    response["challenge"] = b64u_challenge.into();

    Ok(serde_json::to_string(&response)?)
}
