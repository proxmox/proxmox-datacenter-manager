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
