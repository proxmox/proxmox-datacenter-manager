use anyhow::{bail, format_err, Error};
use serde_json::Value;

use proxmox_access_control::types::User;
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
        .insert("add", CliCommand::new(&API_METHOD_ADD_TFA))
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

    let password = if userid != env_userid.as_str() {
        let password = proxmox_sys::linux::tty::read_password("Password: ")?;
        Some(String::from_utf8(password)?)
    } else {
        None
    };

    match r#type {
        TfaType::Recovery => add_recovery(userid, password, description).await,
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
