use anyhow::Error;
use serde_json::Value;

use proxmox_router::cli::{
    format_and_print_result, get_output_format, CliCommand, CliCommandMap, CommandLineInterface,
    OUTPUT_FORMAT,
};
use proxmox_schema::{api, property_string};

use pdm_api_types::{PveRemote, RemoteType, REMOTE_ID_SCHEMA};
use pdm_client::Remote;

use crate::client;

pub fn cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert("list", CliCommand::new(&API_METHOD_LIST_REMOTES))
        .insert("add", CliCommand::new(&API_METHOD_ADD_REMOTE))
        .insert(
            "remove",
            CliCommand::new(&API_METHOD_REMOVE_REMOTE).arg_param(&["id"]),
        )
        .insert(
            "update",
            CliCommand::new(&API_METHOD_UPDATE_REMOTE).arg_param(&["id"]),
        )
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
/// List all the remotes this instance is managing.
async fn list_remotes(param: Value) -> Result<(), Error> {
    let output_format = get_output_format(&param);

    let entries = client()?.list_remotes().await?;

    if output_format == "text" {
        if entries.is_empty() {
            println!("No remotes configured");
            return Ok(());
        }

        for entry in entries {
            match entry {
                Remote::Pve(pve) => {
                    println!("Proxmox VE node {}:", pve.id);
                    println!("    userid: {}", pve.userid);
                    println!("    token: {}", pve.token);
                    if pve.nodes.len() == 1 {
                        println!("    node: {}", property_string::print(&*pve.nodes[0])?);
                    } else {
                        println!("    nodes:");
                        for node in &pve.nodes {
                            println!("        {}", property_string::print(&**node)?);
                        }
                    }
                }
            }
        }
    } else {
        let data = serde_json::to_value(entries)?;
        format_and_print_result(&data, &output_format);
    }
    Ok(())
}

// FIXME: Support `OneOf` in schema so we can just use the `Remote` enum api schema here as input.
#[api(
    input: {
        properties: {
            type: { type: RemoteType },
            remote: {
                flatten: true,
                type: PveRemote,
            },
        }
    }
)]
/// Add a new remote.
async fn add_remote(r#type: RemoteType, remote: pdm_api_types::PveRemote) -> Result<(), Error> {
    let client = client()?;

    match r#type {
        RemoteType::Pve => client.add_remote(&Remote::Pve(remote)).await?,
    }

    Ok(())
}

// FIXME: Support `OneOf` in schema so we can just use the `Remote` enum api schema here as input.
#[api(
    input: {
        properties: {
            id: { schema: REMOTE_ID_SCHEMA },
            updater: {
                flatten: true,
                type: pdm_api_types::PveRemoteUpdater,
            },
        }
    }
)]
/// Update a remote.
async fn update_remote(id: String, updater: pdm_api_types::PveRemoteUpdater) -> Result<(), Error> {
    client()?.update_remote(&id, &updater).await?;
    Ok(())
}

#[api(
    input: {
        properties: {
            id: { schema: REMOTE_ID_SCHEMA },
        }
    }
)]
/// Add a new remote.
async fn remove_remote(id: String) -> Result<(), Error> {
    client()?.remove_remote(&id).await?;
    Ok(())
}
