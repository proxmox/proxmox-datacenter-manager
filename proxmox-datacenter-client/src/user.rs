use anyhow::{bail, format_err, Error};
use serde_json::Value;

use proxmox_router::cli::{
    format_and_print_result, get_output_format, CliCommand, CliCommandMap, CommandLineInterface,
    OUTPUT_FORMAT,
};
use proxmox_schema::api;
use proxmox_tfa::TfaType;

use crate::{client, env};

pub fn cli() -> CommandLineInterface {
    CliCommandMap::new().insert("tfa", tfa_cli()).into()
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
            userid: {
                description: "userid",
                optional: true,
            },
        }
    }
)]
/// List all the remotes this instance is managing.
async fn list_user_tfa(userid: Option<String>, param: Value) -> Result<(), Error> {
    let userid = userid
        .or_else(|| env().userid.clone())
        .ok_or_else(|| format_err!("missing userid and no user logged in?"))?;

    let output_format = get_output_format(&param);

    let entries = client()?.list_user_tfa(&userid).await?;

    if output_format == "text" {
        if entries.is_empty() {
            println!("No TFA entries configured");
            return Ok(());
        }

        for entry in entries {
            let enabled = if entry.info.enable { "✓" } else { "✗" };

            println!("{enabled} {}: {}", entry.ty, entry.info.id);
            // FIXME: print a nicere date...
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

    let password = if userid != env_userid {
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

    let password = if userid != env_userid {
        let password = proxmox_sys::linux::tty::read_password("Password: ")?;
        Some(String::from_utf8(password)?)
    } else {
        None
    };

    client()?
        .remove_tfa_entry(&userid, password.as_deref(), &id)
        .await
}
