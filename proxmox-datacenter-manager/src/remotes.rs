use anyhow::{format_err, Error};
use serde_json::{json, Value};

use proxmox_router::cli::{
    format_and_print_result_full, get_output_format, CliCommand, CliCommandMap,
    CommandLineInterface, OUTPUT_FORMAT,
};
use proxmox_router::{ApiHandler, RpcEnvironment};
use proxmox_schema::{api, property_string};

use pdm_api_types::{PveRemote, RemoteType, REMOTE_ID_SCHEMA};
use pdm_config::remotes::Remote;
use proxmox_datacenter_api as dc_api;

pub fn cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert("list", CliCommand::new(&API_METHOD_LIST_REMOTES))
        .insert("add", CliCommand::new(&API_METHOD_ADD_REMOTE))
        .insert("remove", CliCommand::new(&API_METHOD_REMOVE_REMOTE))
        .insert("update", CliCommand::new(&API_METHOD_UPDATE_REMOTE))
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
            type: { type: RemoteType },
            remote: {
                flatten: true,
                type: PveRemote,
            },
        }
    }
)]
/// Add a new remote.
fn add_remote(
    r#type: RemoteType,
    remote: pdm_api_types::PveRemote,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<(), Error> {
    let mut param = serde_json::to_value(remote)?;
    param["type"] = serde_json::to_value(r#type)?;

    let info = &dc_api::remotes::API_METHOD_ADD_REMOTE;
    match info.handler {
        ApiHandler::Sync(handler) => (handler)(param, info, rpcenv).map(drop),
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
                type: pdm_api_types::PveRemoteUpdater,
            },
        }
    }
)]
/// Update a remote.
fn update_remote(
    id: String,
    updater: pdm_api_types::PveRemoteUpdater,
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
