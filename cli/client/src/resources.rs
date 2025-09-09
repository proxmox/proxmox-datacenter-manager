//! Resources commands.

use std::fmt;

use anyhow::Error;

use proxmox_human_byte::HumanByte;
use proxmox_router::cli::{
    format_and_print_result, CliCommand, CommandLineInterface, OutputFormat,
};
use proxmox_schema::api;

use pdm_api_types::resource::{self, Resource};

use crate::env::use_emoji;
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
    let mut resources = client()?.resources(max_age).await?;
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
                    Resource::PveSdn(r) => println!("{}", PrintResource(r)),
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
        Resource::PveSdn(_) => 4,

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
            ref storage,
            ref node,
            ref status,
        } = self.0;
        write!(
            f,
            "{right}    storage {storage:?} on {node} ({status}) usage: {} of {}",
            HumanByte::new_binary(disk as f64),
            HumanByte::new_binary(maxdisk as f64),
            right = IfWide(
                100,
                Position(90, FractionAsBlock(disk as f64 / maxdisk as f64))
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
            cpu = FractionAsBar(cpu),
            mem = FractionAsBar(mem as f64 / maxmem as f64),
            memcur = HumanByte::new_binary(mem as f64),
            memmax = HumanByte::new_binary(maxmem as f64),
        )
    }
}

impl fmt::Display for PrintResource<resource::PveSdnResource> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "    sdn zone {name} ({status}) on {node}",
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
            cpu = FractionAsBar(cpu),
            mem = FractionAsBar(mem as f64 / maxmem as f64),
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
            right = IfWide(
                100,
                Position(90, FractionAsBlock(disk as f64 / maxdisk as f64))
            ),
        )
    }
}

const COLOR_RESET: &str = "\x1b[0m";

fn color_for_fraction(fraction: f64) -> &'static str {
    const COLOR_LOW: &str = "\x1b[37m";
    const COLOR_MID: &str = "\x1b[97m";
    const COLOR_HI: &str = "\x1b[31m";

    match fraction {
        f if f > 0.9 => COLOR_HI,
        f if f > 0.3 => COLOR_MID,
        _ => COLOR_LOW,
    }
}

/*
/// Render something right-aligned with a known length.
struct Right<T>(T, usize);

impl<T: fmt::Display> fmt::Display for Right<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if !use_emoji() {
            return Ok(());
        }
        let out = self.0.to_string();
        let len = self.1;
        write!(f, "\x1b7\x1b[999C\x1b[{len}D{out}\x1b8")
    }
}
*/

/// Render something at a fixed position from the left.
struct Position<T>(usize, T);

impl<T: fmt::Display> fmt::Display for Position<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if !use_emoji() {
            return Ok(());
        }
        let pos = self.0;
        let out = self.1.to_string();
        write!(f, "\x1b7\x1b[{pos}G{out}\x1b8")
    }
}

struct Formatted<'a, T> {
    format: &'a str,
    content: T,
}

impl<'a, T: fmt::Display> Formatted<'a, T> {
    const fn new(format: &'a str, content: T) -> Self {
        Self { format, content }
    }
}

impl<T: fmt::Display> fmt::Display for Formatted<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if env().use_color() {
            write!(f, "{}{}{}", self.format, self.content, COLOR_RESET)
        } else {
            fmt::Display::fmt(&self.content, f)
        }
    }
}

struct FractionAsBar(f64);

impl fmt::Display for FractionAsBar {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} {}",
            Formatted::new(
                color_for_fraction(self.0),
                format_args!("{:5.1}%", self.0 * 100.),
            ),
            colored_fraction(self.0, fraction_to_bar(self.0)),
        )
    }
}

fn fraction_to_bar(fraction: f64) -> &'static str {
    const BAR_EIGHTS: [&str; 9] = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let index = if fraction < 0.1 {
        0
    } else {
        ((fraction * 8.0).ceil() as usize).min(8)
    };
    BAR_EIGHTS[index]
}

struct FractionAsBlock(f64);

impl fmt::Display for FractionAsBlock {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Since we use "black" as a background color and many terminal color schemes make this
        // the same as the background color, let's also add an "end" of the bar:
        let block_end = if use_emoji() { "▏" } else { "" };

        write!(
            f,
            "{} {}{block_end}",
            Formatted::new(
                color_for_fraction(self.0),
                format_args!("{:5.1}%", self.0 * 100.),
            ),
            colored_fraction(self.0, fraction_to_block(self.0, 3)),
        )
    }
}

fn fraction_to_block(fraction: f64, len: usize) -> String {
    const BLOCK_EIGTHS: [&str; 9] = [" ", "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"];

    let mut out = String::new();
    let perblock = 1f64 / len as f64;
    for block in 0..len {
        let remaining = fraction - perblock * block as f64;
        let scaled = remaining / perblock;
        // +.0001 to ensure inaccuracy errs on the side of 100% rather than 7/8th.
        let index = ((scaled * 9.0001).floor() as usize).min(8);
        out.push_str(BLOCK_EIGTHS[index]);
    }
    out
}

fn colored_fraction<T>(fraction: f64, content: T) -> Formatted<'static, T>
where
    T: fmt::Display,
{
    const FORMAT_LOW: &str = "\x1b[37;40m";
    const FORMAT_MID: &str = "\x1b[97;40m";
    const FORMAT_HI: &str = "\x1b[31;40m";

    let color = match fraction {
        f if f > 0.9 => FORMAT_HI,
        f if f > 0.3 => FORMAT_MID,
        _ => FORMAT_LOW,
    };

    Formatted::new(color, content)
}

/// Only render text if the terminal has a minimum length.
pub struct IfWide<T>(u32, T);

impl<T> fmt::Display for IfWide<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if env::WinSize::cols().is_some_and(|s| s >= self.0) {
            fmt::Display::fmt(&self.1, f)
        } else {
            Ok(())
        }
    }
}
