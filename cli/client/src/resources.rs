//! Resources commands.

use anyhow::Error;

use proxmox_router::cli::{
    format_and_print_result, CliCommand, CommandLineInterface, OutputFormat,
};
use proxmox_schema::api;

use pdm_api_types::resource::{self, Resource};

use crate::{client, env};

pub fn cli() -> CommandLineInterface {
    CliCommand::new(&API_METHOD_GET_RESOURCES).into()
}

#[api(
    input: {
        properties: {
            "max-age": {
                description: "Maximum age of cached remote resources.",
                optional: true,
            },
        }
    }
)]
/// List all the remotes this instance is managing.
async fn get_resources(max_age: Option<u64>) -> Result<(), Error> {
    let resources = client()?.resources(max_age).await?;
    let output_format = env().format_args.output_format;
    if output_format == OutputFormat::Text {
        if resources.is_empty() {
            println!("No resources found.");
            return Ok(());
        }

        for entry in resources {
            println!("Remote {}:", entry.remote);
            if let Some(err) = entry.error {
                println!("    Errors querying the remote: {err}");
            }
            for resource in entry.resources {
                match resource {
                    Resource::PveStorage(r) => {
                        let resource::PveStorageResource {
                            disk,
                            maxdisk,
                            id,
                            storage,
                            node,
                            status,
                        } = r;
                        println!("    storage {storage} ({id}) on {node} ({status})");
                        println!("        usage: {disk}/{maxdisk}");
                    }
                    Resource::PveQemu(r) => {
                        println!(
                            "    pve vm {vmid} {name} ({id}) on {node} ({status})",
                            vmid = r.vmid,
                            name = r.name,
                            id = r.id,
                            node = r.node,
                            status = r.status
                        );
                    }
                    Resource::PveLxc(r) => {
                        println!(
                            "    pve ct {vmid} {name} ({id}) on {node} ({status})",
                            vmid = r.vmid,
                            name = r.name,
                            id = r.id,
                            node = r.node,
                            status = r.status
                        );
                    }
                    Resource::PveNode(r) => {
                        println!("    pve node {node} ({id})", node = r.node, id = r.id,);
                    }
                    Resource::PbsNode(r) => {
                        println!("    pbs node {id}", id = r.id,);
                    }
                    Resource::PbsDatastore(r) => {
                        let resource::PbsDatastoreResource { id, maxdisk, disk } = r;
                        println!("    pbs datastore {id}");
                        println!("        usage: {disk}/{maxdisk}");
                    }
                }
            }
        }
    } else {
        format_and_print_result(&resources, &output_format.to_string());
    }
    Ok(())
}
