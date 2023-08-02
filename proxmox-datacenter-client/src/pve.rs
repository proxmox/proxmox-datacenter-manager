//! PVE node commands.

use anyhow::{bail, Error};
use serde_json::Value;

use proxmox_router::cli::{
    format_and_print_result, format_and_print_result_full, get_output_format, CliCommand,
    CliCommandMap, CommandLineInterface, OUTPUT_FORMAT,
};
use proxmox_schema::{api, ApiType, ArraySchema, ReturnType, Schema};

use pdm_api_types::{RemoteUpid, NODE_SCHEMA, REMOTE_ID_SCHEMA, SNAPSHOT_NAME_SCHEMA, VMID_SCHEMA};

use crate::client;

pub fn cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert("lxc", lxc_cli())
        .insert("node", node_cli())
        .insert("qemu", qemu_cli())
        .insert(
            "resources",
            CliCommand::new(&API_METHOD_CLUSTER_RESOURCES).arg_param(&["remote", "kind"]),
        )
        .insert("task", task_cli())
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
            "config",
            CliCommand::new(&API_METHOD_GET_QEMU_CONFIG).arg_param(&["remote", "vmid"]),
        )
        .insert(
            "list",
            CliCommand::new(&API_METHOD_LIST_QEMU).arg_param(&["remote"]),
        )
        .insert(
            "start",
            CliCommand::new(&API_METHOD_START_QEMU).arg_param(&["remote", "vmid"]),
        )
        .insert(
            "shutdown",
            CliCommand::new(&API_METHOD_SHUTDOWN_QEMU).arg_param(&["remote", "vmid"]),
        )
        .insert(
            "stop",
            CliCommand::new(&API_METHOD_STOP_QEMU).arg_param(&["remote", "vmid"]),
        )
        .into()
}

fn lxc_cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert(
            "config",
            CliCommand::new(&API_METHOD_GET_LXC_CONFIG).arg_param(&["remote", "vmid"]),
        )
        .insert(
            "list",
            CliCommand::new(&API_METHOD_LIST_LXC).arg_param(&["remote"]),
        )
        .insert(
            "start",
            CliCommand::new(&API_METHOD_START_LXC).arg_param(&["remote", "vmid"]),
        )
        .insert(
            "shutdown",
            CliCommand::new(&API_METHOD_SHUTDOWN_LXC).arg_param(&["remote", "vmid"]),
        )
        .insert(
            "stop",
            CliCommand::new(&API_METHOD_STOP_LXC).arg_param(&["remote", "vmid"]),
        )
        .into()
}

fn task_cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert(
            "list",
            CliCommand::new(&API_METHOD_LIST_TASKS).arg_param(&["remote"]),
        )
        .insert(
            "status",
            CliCommand::new(&API_METHOD_TASK_STATUS).arg_param(&["remote", "upid"]),
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
            kind: {
                type: pve_api_types::ClusterResourceKind,
                optional: true,
            },
            remote: { schema: REMOTE_ID_SCHEMA },
        }
    }
)]
/// List all the remotes this instance is managing.
async fn cluster_resources(
    remote: String,
    kind: Option<pve_api_types::ClusterResourceKind>,
    param: Value,
) -> Result<(), Error> {
    const CLUSTER_LIST_SCHEMA: Schema = ArraySchema::new(
        "cluster resources",
        &pve_api_types::ClusterResource::API_SCHEMA,
    )
    .schema();

    let output_format = get_output_format(&param);

    let data = client()?.pve_cluster_resources(&remote, kind).await?;

    format_and_print_result_full(
        &mut serde_json::to_value(data)?,
        &ReturnType {
            optional: false,
            schema: &CLUSTER_LIST_SCHEMA,
        },
        &output_format,
        &Default::default(),
    );
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
            vmid: { schema: VMID_SCHEMA },
            state: {
                type: pdm_api_types::ConfigurationState,
                optional: true,
            },
            snapshot: {
                schema: SNAPSHOT_NAME_SCHEMA,
                optional: true,
            },
        }
    }
)]
/// List all the remotes this instance is managing.
async fn get_qemu_config(
    remote: String,
    node: Option<String>,
    vmid: u32,
    state: Option<pdm_api_types::ConfigurationState>,
    snapshot: Option<String>,
    param: Value,
) -> Result<(), Error> {
    let output_format = get_output_format(&param);

    let config = client()?
        .pve_qemu_config(
            &remote,
            node.as_deref(),
            vmid,
            state.unwrap_or_default(),
            snapshot.as_deref(),
        )
        .await?;

    if output_format == "text" {
        println!("{}", serde_json::to_string_pretty(&config)?);
    } else {
        format_and_print_result(&config, &output_format);
    }
    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA },
        }
    }
)]
/// List all the remotes this instance is managing.
async fn start_qemu(remote: String, node: Option<String>, vmid: u32) -> Result<(), Error> {
    let client = client()?;
    let upid = client
        .pve_qemu_start(&remote, node.as_deref(), vmid)
        .await?;
    println!("upid: {upid}");
    let status = client.pve_wait_for_task(&upid).await?;
    println!("{status:#?}");

    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA },
        }
    }
)]
/// List all the remotes this instance is managing.
async fn shutdown_qemu(remote: String, node: Option<String>, vmid: u32) -> Result<(), Error> {
    let client = client()?;
    let upid = client
        .pve_qemu_shutdown(&remote, node.as_deref(), vmid)
        .await?;
    println!("upid: {upid}");
    let status = client.pve_wait_for_task(&upid).await?;
    println!("{status:#?}");

    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA },
        }
    }
)]
/// List all the remotes this instance is managing.
async fn stop_qemu(remote: String, node: Option<String>, vmid: u32) -> Result<(), Error> {
    let client = client()?;
    let upid = client.pve_qemu_stop(&remote, node.as_deref(), vmid).await?;
    println!("upid: {upid}");
    let status = client.pve_wait_for_task(&upid).await?;
    println!("{status:#?}");

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
            vmid: { schema: VMID_SCHEMA },
            state: {
                type: pdm_api_types::ConfigurationState,
                optional: true,
            },
            snapshot: {
                schema: SNAPSHOT_NAME_SCHEMA,
                optional: true,
            },
        }
    }
)]
/// List all the remotes this instance is managing.
async fn get_lxc_config(
    remote: String,
    node: Option<String>,
    vmid: u32,
    state: Option<pdm_api_types::ConfigurationState>,
    snapshot: Option<String>,
    param: Value,
) -> Result<(), Error> {
    let output_format = get_output_format(&param);

    let config = client()?
        .pve_lxc_config(
            &remote,
            node.as_deref(),
            vmid,
            state.unwrap_or_default(),
            snapshot.as_deref(),
        )
        .await?;

    if output_format == "text" {
        println!("{}", serde_json::to_string_pretty(&config)?);
    } else {
        format_and_print_result(&config, &output_format);
    }
    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA },
        }
    }
)]
/// List all the remotes this instance is managing.
async fn start_lxc(remote: String, node: Option<String>, vmid: u32) -> Result<(), Error> {
    let client = client()?;
    let upid = client.pve_lxc_start(&remote, node.as_deref(), vmid).await?;
    println!("upid: {upid}");
    let status = client.pve_wait_for_task(&upid).await?;
    println!("{status:#?}");

    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA },
        }
    }
)]
/// List all the remotes this instance is managing.
async fn shutdown_lxc(remote: String, node: Option<String>, vmid: u32) -> Result<(), Error> {
    let client = client()?;
    let upid = client
        .pve_lxc_shutdown(&remote, node.as_deref(), vmid)
        .await?;
    println!("upid: {upid}");
    let status = client.pve_wait_for_task(&upid).await?;
    println!("{status:#?}");

    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA },
        }
    }
)]
/// List all the remotes this instance is managing.
async fn stop_lxc(remote: String, node: Option<String>, vmid: u32) -> Result<(), Error> {
    let client = client()?;
    let upid = client.pve_lxc_stop(&remote, node.as_deref(), vmid).await?;
    println!("upid: {upid}");
    let status = client.pve_wait_for_task(&upid).await?;
    println!("{status:#?}");

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
async fn list_tasks(remote: String, node: Option<String>, param: Value) -> Result<(), Error> {
    const TASK_LIST_SCHEMA: Schema =
        ArraySchema::new("task list", &pve_api_types::ListTasksResponse::API_SCHEMA).schema();

    let output_format = get_output_format(&param);

    let data = client()?.pve_list_tasks(&remote, node.as_deref()).await?;

    format_and_print_result_full(
        &mut serde_json::to_value(data)?,
        &ReturnType {
            optional: false,
            schema: &TASK_LIST_SCHEMA,
        },
        &output_format,
        &proxmox_router::cli::default_table_format_options(),
    );
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
            upid: { type: RemoteUpid },
        }
    }
)]
/// List all the remotes this instance is managing.
async fn task_status(remote: String, upid: RemoteUpid, param: Value) -> Result<(), Error> {
    let output_format = get_output_format(&param);

    if remote != upid.remote() {
        bail!("mismatching remote in upid");
    }
    let data = client()?.pve_task_status(&upid).await?;

    format_and_print_result_full(
        &mut serde_json::to_value(data)?,
        &ReturnType {
            optional: false,
            schema: &pve_api_types::TaskStatus::API_SCHEMA,
        },
        &output_format,
        &Default::default(),
    );
    Ok(())
}
