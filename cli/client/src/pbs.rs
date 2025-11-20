//! PBS node commands.

use anyhow::Error;

use pbs_api_types::DATASTORE_SCHEMA;
use proxmox_router::cli::{
    format_and_print_result, format_and_print_result_full, CliCommand, CliCommandMap,
    CommandLineInterface, OutputFormat,
};
use proxmox_rrd_api_types::{RrdMode, RrdTimeframe};
use proxmox_schema::{api, ApiType, ArraySchema, ReturnType, Schema};

use pdm_api_types::remotes::REMOTE_ID_SCHEMA;

use crate::{client, env};

pub fn cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert("datastore", datastore_cli())
        .insert("snapshot", snapshot_cli())
        .insert("node", node_cli())
        .insert("task", task_cli())
        .into()
}

fn datastore_cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert(
            "list",
            CliCommand::new(&API_METHOD_LIST_DATASTORES).arg_param(&["remote"]),
        )
        .insert(
            "rrddata",
            CliCommand::new(&API_METHOD_GET_PBS_DATASTORE_RRD_DATA).arg_param(&[
                "remote",
                "datastore",
                "mode",
                "timeframe",
            ]),
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

fn node_cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert(
            "rrddata",
            CliCommand::new(&API_METHOD_GET_PBS_NODE_RRD_DATA).arg_param(&[
                "remote",
                "mode",
                "timeframe",
            ]),
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

#[api(
    input: {
        properties: {
            remote: {
                schema: REMOTE_ID_SCHEMA
            },
            mode: {
                type: RrdMode
            },
            timeframe: {
                type: RrdTimeframe
            }
        }
    }
)]
/// Return a PBS node's metric data.
async fn get_pbs_node_rrd_data(
    remote: String,
    mode: RrdMode,
    timeframe: RrdTimeframe,
) -> Result<(), Error> {
    let config = client()?.pbs_node_rrddata(&remote, mode, timeframe).await?;

    let output_format = env().format_args.output_format;
    if output_format == OutputFormat::Text {
        println!("{}", serde_json::to_string_pretty(&config)?);
    } else {
        format_and_print_result(&config, &output_format.to_string());
    }
    Ok(())
}

#[api(
    input: {
        properties: {
            remote: {
                schema: REMOTE_ID_SCHEMA
            },
            datastore: {
                schema: DATASTORE_SCHEMA,
            },
            mode: {
                type: RrdMode
            },
            timeframe: {
                type: RrdTimeframe
            }
        }
    }
)]
/// Return a PBS datastore's metric data.
async fn get_pbs_datastore_rrd_data(
    remote: String,
    datastore: String,
    mode: RrdMode,
    timeframe: RrdTimeframe,
) -> Result<(), Error> {
    let config = client()?
        .pbs_datastore_rrddata(&remote, &datastore, mode, timeframe)
        .await?;

    let output_format = env().format_args.output_format;
    if output_format == OutputFormat::Text {
        println!("{}", serde_json::to_string_pretty(&config)?);
    } else {
        format_and_print_result(&config, &output_format.to_string());
    }
    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
        }
    }
)]
/// List the tasks of a cluster.
async fn list_tasks(remote: String) -> Result<(), Error> {
    const TASK_LIST_SCHEMA: Schema =
        ArraySchema::new("task list", &pbs_api_types::TaskListItem::API_SCHEMA).schema();

    let data = client()?.pbs_list_tasks(&remote).await?;

    format_and_print_result_full(
        &mut serde_json::to_value(data)?,
        &ReturnType {
            optional: false,
            schema: &TASK_LIST_SCHEMA,
        },
        &env().format_args.output_format.to_string(),
        &proxmox_router::cli::default_table_format_options(),
    );
    Ok(())
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            upid: {
                description: "The task UPID, optionally with the remote name prefix",
            },
        }
    }
)]
/// Query the status of a task.
async fn task_status(remote: String, upid: String) -> Result<(), Error> {
    let upid = crate::upid::parse_for_remote(Some(&remote), &upid)?;
    let data = client()?.pbs_task_status(&upid).await?;

    format_and_print_result_full(
        &mut serde_json::to_value(data)?,
        &ReturnType {
            optional: false,
            schema: &pdm_api_types::pbs::TaskStatus::API_SCHEMA,
        },
        &env().format_args.output_format.to_string(),
        &Default::default(),
    );
    Ok(())
}
