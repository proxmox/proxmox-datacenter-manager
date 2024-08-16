//! PBS node commands.

use anyhow::Error;

use proxmox_router::cli::{
    format_and_print_result, CliCommand, CliCommandMap, CommandLineInterface, OutputFormat,
};
use proxmox_schema::api;

use pdm_api_types::remotes::REMOTE_ID_SCHEMA;

use crate::{client, env};

pub fn cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert("datastore", datastore_cli())
        .insert("snapshot", snapshot_cli())
        .into()
}

fn datastore_cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert(
            "list",
            CliCommand::new(&API_METHOD_LIST_DATASTORES).arg_param(&["remote"]),
        )
        .into()
}

fn snapshot_cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert(
            "list",
            CliCommand::new(&API_METHOD_LIST_SNAPSHOTS).arg_param(&["remote", "datastore"]),
        )
        .into()
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
        }
    }
)]
/// List all the remotes this instance is managing.
async fn list_datastores(remote: String) -> Result<(), Error> {
    let mut entries = client()?.pbs_list_datastores(&remote).await?;

    let output_format = env().format_args.output_format;
    if output_format == OutputFormat::Text {
        if entries.is_empty() {
            println!("No datastores configured.");
            return Ok(());
        }

        entries.sort_by(|a, b| a.name.cmp(&b.name));
        for entry in entries {
            println!("{}:", entry.name);
            if let Some(comment) = entry.comment {
                println!("    {comment}");
            }
        }
    } else {
        format_and_print_result(&entries, &output_format.to_string());
    }
    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            datastore: { schema: pbs_api_types::DATASTORE_SCHEMA },
            ns: {
                schema: pbs_api_types::BACKUP_NAMESPACE_SCHEMA,
                optional: true,
            },
        }
    }
)]
/// List all the remotes this instance is managing.
async fn list_snapshots(
    remote: String,
    datastore: String,
    ns: Option<String>,
) -> Result<(), Error> {
    let mut entries = client()?
        .pbs_list_snapshots(&remote, &datastore, ns.as_deref())
        .await?;

    let output_format = env().format_args.output_format;
    if output_format == OutputFormat::Text {
        if entries.is_empty() {
            println!("No datastores configured.");
            return Ok(());
        }

        entries.sort_by(|a, b| a.backup.cmp(&b.backup));
        for entry in entries {
            let protected = if entry.protected { " (protected)" } else { "" };
            println!("{name}:{protected}", name = entry.backup);
            if let Some(owner) = entry.owner {
                println!("    owner: {owner}");
            }
            if let Some(comment) = entry.comment {
                println!("    {comment}");
            }
        }
    } else {
        format_and_print_result(&entries, &output_format.to_string());
    }
    Ok(())
}
