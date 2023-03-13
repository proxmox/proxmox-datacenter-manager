use anyhow::{format_err, Error};
use serde_json::Value;

use proxmox_router::cli::{
    get_output_format, CliCommand, CliCommandMap, CommandLineInterface, OUTPUT_FORMAT,
};
use proxmox_router::{ApiHandler, RpcEnvironment};
use proxmox_schema::{api, property_string};

use pdm_config::remotes::Remote;
use proxmox_datacenter_api as dc_api;

pub fn cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert("list", CliCommand::new(&API_METHOD_LIST_REMOTES))
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
        todo!();
    }
    Ok(())
}
