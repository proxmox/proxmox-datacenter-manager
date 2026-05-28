//! Resources commands.

use std::fmt;

use anyhow::Error;

use proxmox_human_byte::HumanByte;
use proxmox_router::cli::{
    format_and_print_result, CliCommand, CommandLineInterface, OutputFormat,
};
use proxmox_schema::api;

use pdm_api_types::resource::{self, Resource};

use crate::{client, env};

use crate::term;

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
    let mut resources = client()?.resources(max_age, None).await?;
    let output_format = env().format_args.output_format;
    if output_format == OutputFormat::Text {
        if resources.is_empty() {
            println!("No resources found.");
            return Ok(());
        }

        resources.sort_by(|a, b| a.remote.cmp(&b.remote));
        for mut entry in resources {
            println!("Remote {}:", entry.remote);
            if let Some(err) = entry.error {
                println!("    Errors querying the remote: {err}");
            }
            entry.resources.sort_by_key(resource_order);
            for resource in entry.resources {
                match resource {
                    Resource::PveStorage(r) => println!("{}", PrintResource(r)),
                    Resource::PveQemu(r) => println!("{}", PrintResource(r)),
                    Resource::PveLxc(r) => println!("{}", PrintResource(r)),
                    Resource::PveNode(r) => println!("{}", PrintResource(r)),
                    Resource::PveNetwork(r) => println!("{}", PrintResource(r)),
                    Resource::PbsNode(r) => println!("{}", PrintResource(r)),
                    Resource::PbsDatastore(r) => println!("{}", PrintResource(r)),
                }
            }
        }
    } else {
        format_and_print_result(&resources, &output_format.to_string());
    }
    Ok(())
}

fn resource_order(item: &Resource) -> usize {
    match item {
        Resource::PveNode(_) => 0,
        Resource::PveStorage(_) => 1,
        Resource::PveLxc(_) => 2,
        Resource::PveQemu(_) => 3,
        Resource::PveNetwork(_) => 4,

        Resource::PbsNode(_) => 0,
        Resource::PbsDatastore(_) => 1,
    }
}

struct PrintResource<R>(R);

impl fmt::Display for PrintResource<resource::PveStorageResource> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let resource::PveStorageResource {
            disk,
            maxdisk,
            id: _,
            shared: _,
            ref storage,
            ref node,
            ref status,
        } = self.0;
        write!(
            f,
            "{right}    storage {storage:?} on {node} ({status}) usage: {} of {}",
            HumanByte::new_binary(disk as f64),
            HumanByte::new_binary(maxdisk as f64),
            right = term::IfWide(
                100,
                term::Position(90, term::FractionAsBlock(disk as f64 / maxdisk as f64))
            )
        )?;
        Ok(())
    }
}

impl fmt::Display for PrintResource<resource::PveQemuResource> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "    vm {vmid} ({status}) {name} on {node}",
            vmid = self.0.vmid,
            name = self.0.name,
            node = self.0.node,
            status = self.0.status
        )
    }
}

impl fmt::Display for PrintResource<resource::PveLxcResource> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "    ct {vmid} ({status}) {name} on {node}",
            vmid = self.0.vmid,
            name = self.0.name,
            node = self.0.node,
            status = self.0.status
        )
    }
}

impl fmt::Display for PrintResource<resource::PveNodeResource> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let resource::PveNodeResource {
            ref node,
            cpu,
            maxcpu: _,
            mem,
            maxmem,
            ..
        } = self.0;
        write!(
            f,
            "    node {node} cpu: {cpu}, mem: {mem} ({memcur} of {memmax})",
            cpu = term::FractionAsBar(cpu),
            mem = term::FractionAsBar(mem as f64 / maxmem as f64),
            memcur = HumanByte::new_binary(mem as f64),
            memmax = HumanByte::new_binary(maxmem as f64),
        )
    }
}

impl fmt::Display for PrintResource<resource::PveNetworkResource> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "    network {network_type} {name} ({status}) on {node}",
            network_type = self.0.network_type(),
            name = self.0.name(),
            status = self.0.status(),
            node = self.0.node(),
        )
    }
}

impl fmt::Display for PrintResource<resource::PbsNodeResource> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let resource::PbsNodeResource {
            cpu,
            maxcpu: _,
            ref name,
            mem,
            maxmem,
            ..
        } = self.0;
        writeln!(f, "    Node {name}")?;
        write!(
            f,
            "        cpu: {cpu}, mem: {mem} ({memcur} of {memmax})",
            cpu = term::FractionAsBar(cpu),
            mem = term::FractionAsBar(mem as f64 / maxmem as f64),
            memcur = HumanByte::new_binary(mem as f64),
            memmax = HumanByte::new_binary(maxmem as f64),
        )?;
        Ok(())
    }
}

impl fmt::Display for PrintResource<resource::PbsDatastoreResource> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let resource::PbsDatastoreResource {
            ref name,
            maxdisk,
            disk,
            ..
        } = self.0;

        write!(
            f,
            "{right}    Datastore {name} usage: {} of {}",
            HumanByte::new_binary(disk as f64),
            HumanByte::new_binary(maxdisk as f64),
            right = term::IfWide(
                100,
                term::Position(90, term::FractionAsBlock(disk as f64 / maxdisk as f64))
            ),
        )
    }
}
