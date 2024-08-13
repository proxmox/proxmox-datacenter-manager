use anyhow::Error;

use proxmox_router::cli::{
    format_and_print_result, format_and_print_result_full, CliCommand, CliCommandMap,
    CommandLineInterface, OutputFormat,
};
use proxmox_schema::{api, property_string, ApiType, ReturnType};

use pdm_api_types::remotes::{Remote, RemoteType, RemoteUpdater, REMOTE_ID_SCHEMA};

use crate::{client, env};

pub fn cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert("list", CliCommand::new(&API_METHOD_LIST_REMOTES))
        .insert(
            "add",
            CliCommand::new(&API_METHOD_ADD_REMOTE).arg_param(&["type", "id"]),
        )
        .insert(
            "delete",
            CliCommand::new(&API_METHOD_DELETE_REMOTE).arg_param(&["id"]),
        )
        .insert(
            "update",
            CliCommand::new(&API_METHOD_UPDATE_REMOTE).arg_param(&["id"]),
        )
        .insert(
            "version",
            CliCommand::new(&API_METHOD_REMOTE_VERSION).arg_param(&["id"]),
        )
        .into()
}

#[api]
/// List all the remotes this instance is managing.
async fn list_remotes() -> Result<(), Error> {
    let entries = client()?.list_remotes().await?;

    let output_format = env().format_args.output_format;
    if output_format == OutputFormat::Text {
        if entries.is_empty() {
            println!("No remotes configured");
            return Ok(());
        }

        for entry in entries {
            match entry.ty {
                RemoteType::Pve => println!("Proxmox VE node {}:", entry.id),
                RemoteType::Pbs => println!("Proxmox Backup Server node {}:", entry.id),
            }
            println!("    auth id: {}", entry.authid);
            println!("    token: {}", entry.token);
            if entry.nodes.len() == 1 {
                println!("    node: {}", property_string::print(&*entry.nodes[0])?);
            } else {
                println!("    nodes:");
                for node in &entry.nodes {
                    println!("        {}", property_string::print(&**node)?);
                }
            }
        }
    } else {
        format_and_print_result(&entries, &output_format.to_string());
    }
    Ok(())
}

// FIXME: Support `OneOf` in schema so we can just use the `Remote` enum api schema here as input.
#[api(
    input: {
        properties: {
            entry: {
                flatten: true,
                type: Remote,
            },
        }
    }
)]
/// Add a new remote.
async fn add_remote(entry: Remote) -> Result<(), Error> {
    client()?.add_remote(&entry).await?;
    Ok(())
}

// FIXME: Support `OneOf` in schema so we can just use the `Remote` enum api schema here as input.
#[api(
    input: {
        properties: {
            id: { schema: REMOTE_ID_SCHEMA },
            updater: {
                flatten: true,
                type: RemoteUpdater,
            },
        }
    }
)]
/// Update a remote.
async fn update_remote(id: String, updater: RemoteUpdater) -> Result<(), Error> {
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
async fn delete_remote(id: String) -> Result<(), Error> {
    client()?.delete_remote(&id).await?;
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
async fn remote_version(id: String) -> Result<(), Error> {
    let data = client()?.remote_version(&id).await?;
    format_and_print_result_full(
        &mut serde_json::to_value(data)?,
        &ReturnType {
            optional: false,
            schema: &pve_api_types::VersionResponse::API_SCHEMA,
        },
        &env().format_args.output_format.to_string(),
        &Default::default(),
    );
    Ok(())
}
