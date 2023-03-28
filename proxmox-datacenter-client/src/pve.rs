//! PVE node commands.

use anyhow::Error;
use serde_json::Value;

use proxmox_router::cli::{
    format_and_print_result, get_output_format, CliCommand, CliCommandMap, CommandLineInterface,
    OUTPUT_FORMAT,
};
use proxmox_schema::api;

use pdm_api_types::{NODE_SCHEMA, REMOTE_ID_SCHEMA};

use crate::client;

pub fn cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert("lxc", lxc_cli())
        .insert("node", node_cli())
        .insert("qemu", qemu_cli())
        .into()
}

fn node_cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert(
            "list",
            CliCommand::new(&API_METHOD_LIST_NODES).arg_param(&["remote"]),
        )
        .into()
}

fn qemu_cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert(
            "list",
            CliCommand::new(&API_METHOD_LIST_QEMU).arg_param(&["remote"]),
        )
        .into()
}

fn lxc_cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert(
            "list",
            CliCommand::new(&API_METHOD_LIST_LXC).arg_param(&["remote"]),
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
            println!("No nodes found?");
            return Ok(());
        }

        for entry in entries {
            println!("{}: {}", entry.node, entry.status);
        }
    } else {
        format_and_print_result(&entries, &output_format);
    }
    Ok(())
}

#[api(
    input: {
        properties: {
            "output-format": {
                schema: OUTPUT_FORMAT,
                optional: true,
            },
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
        }
    }
)]
/// List all the remotes this instance is managing.
async fn list_qemu(remote: String, node: Option<String>, param: Value) -> Result<(), Error> {
    let output_format = get_output_format(&param);

    let entries = client()?.pve_list_qemu(&remote, node.as_deref()).await?;

    if output_format == "text" {
        if entries.is_empty() {
            println!("No vms available");
            return Ok(());
        }

        for entry in entries {
            println!("{}: {}", entry.vmid, entry.status);
        }
    } else {
        format_and_print_result(&entries, &output_format);
    }
    Ok(())
}

#[api(
    input: {
        properties: {
            "output-format": {
                schema: OUTPUT_FORMAT,
                optional: true,
            },
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
        }
    }
)]
/// List all the remotes this instance is managing.
async fn list_lxc(remote: String, node: Option<String>, param: Value) -> Result<(), Error> {
    let output_format = get_output_format(&param);

    let entries = client()?.pve_list_lxc(&remote, node.as_deref()).await?;

    if output_format == "text" {
        if entries.is_empty() {
            println!("No containers available");
            return Ok(());
        }

        for entry in entries {
            println!("{}: {}", entry.vmid, entry.status);
        }
    } else {
        format_and_print_result(&entries, &output_format);
    }
    Ok(())
}
