//! PVE node commands.

use anyhow::Error;
use serde_json::Value;

use proxmox_router::cli::{
    format_and_print_result, format_and_print_result_full, get_output_format, CliCommand,
    CliCommandMap, CommandLineInterface, OUTPUT_FORMAT,
};
use proxmox_schema::{api, property_string, ApiType, ReturnType};

use pdm_api_types::{Remote, REMOTE_ID_SCHEMA};

use crate::client;

pub fn cli() -> CommandLineInterface {
    CliCommandMap::new().insert("node", node_cli()).into()
}

fn node_cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert(
            "list",
            CliCommand::new(&API_METHOD_LIST_NODES).arg_param(&["remote"]),
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
            remote: { schema: REMOTE_ID_SCHEMA },
        }
    }
)]
/// List all the remotes this instance is managing.
async fn list_nodes(remote: String, param: Value) -> Result<(), Error> {
    let output_format = get_output_format(&param);

    let entries = client()?.pve_list_nodes(&remote).await?;

    if output_format == "text" {
        if entries.is_empty() {
            println!("No remotes configured");
            return Ok(());
        }

        for entry in entries {
            println!("{}: {}", entry.node, entry.status);
        }
    } else {
        let data = serde_json::to_value(entries)?;
        format_and_print_result(&data, &output_format);
    }
    Ok(())
}
