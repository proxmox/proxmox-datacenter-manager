use anyhow::{format_err, Error};
use serde_json::{json, Value};

use proxmox_router::cli::{
    format_and_print_result_full, get_output_format, CliCommand, CliCommandMap,
    CommandLineInterface, OUTPUT_FORMAT,
};
use proxmox_router::{ApiHandler, RpcEnvironment};
use proxmox_schema::{api, property_string};

use pdm_api_types::remotes::{Remote, RemoteType, RemoteUpdater, REMOTE_ID_SCHEMA};
use pdm_api_types::subscription::{RemoteSubscriptionState, RemoteSubscriptions};
use server::api as dc_api;

pub fn cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert("list", CliCommand::new(&API_METHOD_LIST_REMOTES))
        .insert(
            "subscriptions",
            CliCommand::new(&API_METHOD_GET_REMOTE_SUBSCRIPTIONS),
        )
        .insert("add", CliCommand::new(&API_METHOD_ADD_REMOTE))
        .insert(
            "remove",
            CliCommand::new(&API_METHOD_REMOVE_REMOTE).arg_param(&["id"]),
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
fn list_remotes(param: Value, rpcenv: &mut dyn RpcEnvironment) -> Result<(), Error> {
    let output_format = get_output_format(&param);

    let info = &dc_api::remotes::API_METHOD_LIST_REMOTES;
    let data = match info.handler {
        ApiHandler::Sync(handler) => (handler)(param, info, rpcenv)?,
        _ => unreachable!(),
    };

    if output_format == "text" {
        let entries: Vec<Remote> = serde_json::from_value(data)
            .map_err(|err| format_err!("list_remotes api call returned invalid data - {err}"))?;

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
        //format_and_print_result(&data, &output_format);
        let mut data = data;
        format_and_print_result_full(
            &mut data,
            &info.returns,
            &output_format,
            &Default::default(),
        );
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
        }
    }
)]
/// Get the status of all the remotes this instance is managing.
async fn get_remote_subscriptions(
    mut param: Value,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<(), Error> {
    let output_format = get_output_format(&param);

    param["verbose"] = serde_json::Value::Bool(true);

    let info = &dc_api::resources::API_METHOD_GET_SUBSCRIPTION_STATUS;
    let data = match info.handler {
        ApiHandler::Async(handler) => (handler)(param, info, rpcenv).await?,
        _ => unreachable!(),
    };

    if output_format == "text" {
        let entries: Vec<RemoteSubscriptions> = serde_json::from_value(data)
            .map_err(|err| format_err!("list_remotes api call returned invalid data - {err}"))?;

        if entries.is_empty() {
            println!("No remotes configured");
            return Ok(());
        }

        let mut first = true;
        for entry in entries {
            let state = match entry.state {
                RemoteSubscriptionState::None => "None",
                RemoteSubscriptionState::Unknown => "Unknown",
                RemoteSubscriptionState::Mixed => "Mixed",
                RemoteSubscriptionState::Active => "Active",
            };
            let ln = if first { "" } else { "\n" };
            first = false;
            println!("{ln}Remote {} subscription status: {state}", entry.remote);
            if let Some(error) = entry.error {
                println!("    Errror: {error}");
            }
            if let Some(node_status) = entry.node_status {
                for (node, status) in &node_status {
                    let status = match status.as_ref().map(property_string::print) {
                        Some(Ok(status)) => status,
                        Some(Err(err)) => err.to_string(),
                        None => "None".to_owned(),
                    };
                    println!("    node {node}: {status}");
                }
            }
        }
    } else {
        //format_and_print_result(&data, &output_format);
        let mut data = data;
        format_and_print_result_full(
            &mut data,
            &info.returns,
            &output_format,
            &Default::default(),
        );
    }
    Ok(())
}

// FIXME: Support `OneOf` in schema so we can just use the `Remote` enum api schema here as input.
#[api(
    input: {
        properties: {
            remote: {
                flatten: true,
                type: Remote,
            },
        }
    }
)]
/// Add a new remote.
async fn add_remote(param: Value, rpcenv: &mut dyn RpcEnvironment) -> Result<(), Error> {
    let info = &dc_api::remotes::API_METHOD_ADD_REMOTE;
    match info.handler {
        ApiHandler::Async(handler) => (handler)(param, info, rpcenv).await.map(drop),
        _ => unreachable!(),
    }
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
fn update_remote(
    id: String,
    updater: RemoteUpdater,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<(), Error> {
    let mut param = serde_json::to_value(updater)?;
    param["id"] = id.into();

    let info = &dc_api::remotes::API_METHOD_UPDATE_REMOTE;
    match info.handler {
        ApiHandler::Sync(handler) => (handler)(param, info, rpcenv).map(drop),
        _ => unreachable!(),
    }
}

#[api(
    input: {
        properties: {
            id: { schema: REMOTE_ID_SCHEMA },
        }
    }
)]
/// Add a new remote.
fn remove_remote(id: String, rpcenv: &mut dyn RpcEnvironment) -> Result<(), Error> {
    let param = json!({ "id": id });

    let info = &dc_api::remotes::API_METHOD_REMOVE_REMOTE;
    match info.handler {
        ApiHandler::Sync(handler) => (handler)(param, info, rpcenv).map(drop),
        _ => unreachable!(),
    }
}

#[api(
    input: {
        properties: {
            id: { schema: REMOTE_ID_SCHEMA },
        }
    }
)]
/// Add a new remote.
async fn remote_version(param: Value, rpcenv: &mut dyn RpcEnvironment) -> Result<(), Error> {
    let output_format = get_output_format(&param);

    let info = &dc_api::remotes::API_METHOD_VERSION;
    let mut data = match info.handler {
        ApiHandler::Async(handler) => (handler)(param, info, rpcenv).await?,
        _ => unreachable!(),
    };

    format_and_print_result_full(
        &mut data,
        &info.returns,
        &output_format,
        &Default::default(),
    );

    Ok(())
}
