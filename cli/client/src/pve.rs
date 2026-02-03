//! PVE node commands.

use std::fmt;
use std::time::Duration;

use anyhow::{format_err, Error};

use proxmox_router::cli::{
    format_and_print_result, format_and_print_result_full, CliCommand, CliCommandMap,
    CommandLineInterface, OutputFormat,
};
use proxmox_rrd_api_types::{RrdMode, RrdTimeframe};
use proxmox_schema::{api, ApiType, ArraySchema, ReturnType, Schema};

use pdm_api_types::remotes::REMOTE_ID_SCHEMA;
use pdm_api_types::{CIDR_FORMAT, NODE_SCHEMA, SNAPSHOT_NAME_SCHEMA, VMID_SCHEMA};
use pve_api_types::StartQemuMigrationType;

use crate::{client, env};

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
        .insert(
            "rrddata",
            CliCommand::new(&API_METHOD_GET_NODE_RRD_DATA).arg_param(&[
                "remote",
                "node",
                "mode",
                "timeframe",
            ]),
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
        .insert(
            "migrate",
            CliCommand::new(&API_METHOD_MIGRATE_QEMU).arg_param(&["remote", "vmid", "target"]),
        )
        .insert(
            "remote-migrate",
            CliCommand::new(&API_METHOD_REMOTE_MIGRATE_QEMU)
                .arg_param(&["remote", "vmid", "target"]),
        )
        .insert(
            "rrddata",
            CliCommand::new(&API_METHOD_GET_QEMU_RRD_DATA).arg_param(&[
                "remote",
                "vmid",
                "mode",
                "timeframe",
            ]),
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
        .insert(
            "migrate",
            CliCommand::new(&API_METHOD_MIGRATE_LXC).arg_param(&["remote", "vmid", "target"]),
        )
        .insert(
            "remote-migrate",
            CliCommand::new(&API_METHOD_REMOTE_MIGRATE_LXC)
                .arg_param(&["remote", "vmid", "target"]),
        )
        .insert(
            "rrddata",
            CliCommand::new(&API_METHOD_GET_LXC_RRD_DATA).arg_param(&[
                "remote",
                "vmid",
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
/// List all the nodes of a pve cluster.
async fn list_nodes(remote: String) -> Result<(), Error> {
    let mut entries = client()?.pve_list_nodes(&remote).await?;

    let output_format = env().format_args.output_format;
    if output_format == OutputFormat::Text {
        if entries.is_empty() {
            println!("No nodes found?");
            return Ok(());
        }

        entries.sort_by(|a, b| a.node.cmp(&b.node));
        for entry in entries {
            println!("{}: {}", entry.node, entry.status);
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
            node: {
                schema: NODE_SCHEMA,
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
/// Return a PVE node's metric data.
async fn get_node_rrd_data(
    remote: String,
    node: String,
    mode: RrdMode,
    timeframe: RrdTimeframe,
) -> Result<(), Error> {
    let config = client()?
        .pve_node_rrddata(&remote, &node, mode, timeframe)
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
            kind: {
                type: pve_api_types::ClusterResourceKind,
                optional: true,
            },
            remote: { schema: REMOTE_ID_SCHEMA },
        }
    }
)]
/// Query the cluster resources.
async fn cluster_resources(
    remote: String,
    kind: Option<pve_api_types::ClusterResourceKind>,
) -> Result<(), Error> {
    const CLUSTER_LIST_SCHEMA: Schema = ArraySchema::new(
        "cluster resources",
        &pve_api_types::ClusterResource::API_SCHEMA,
    )
    .schema();

    let data = client()?.pve_cluster_resources(&remote, kind).await?;

    format_and_print_result_full(
        &mut serde_json::to_value(data)?,
        &ReturnType {
            optional: false,
            schema: &CLUSTER_LIST_SCHEMA,
        },
        &env().format_args.output_format.to_string(),
        &Default::default(),
    );
    Ok(())
}

struct FormatTagList<'a>(Vec<&'a str>);

impl fmt::Display for FormatTagList<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut tag_sep = " [";
        for tag in &self.0 {
            let (color_owned, reset_owned);
            let (mut color, mut reset) = ("", "");
            if env().use_color() {
                if let Some(rgb) = pdm_ui_shared::colors::text_to_rgb(tag) {
                    (color_owned, reset_owned) = (
                        rgb.as_ansi().to_string(),
                        pdm_ui_shared::colors::TtyResetColor.to_string(),
                    );
                    color = &color_owned;
                    reset = &reset_owned;
                }
            }
            write!(f, "{tag_sep}{color}{tag}{reset}")?;
            tag_sep = ", ";
        }
        if tag_sep == ", " {
            f.write_str("]")?;
        }
        Ok(())
    }
}

#[api(
    input: {
        properties: {
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
        }
    }
)]
/// List the QEMU VMs of a cluster.
async fn list_qemu(remote: String, node: Option<String>) -> Result<(), Error> {
    let mut entries = client()?.pve_list_qemu(&remote, node.as_deref()).await?;

    let output_format = env().format_args.output_format;
    if output_format == OutputFormat::Text {
        if entries.is_empty() {
            println!("No vms available");
            return Ok(());
        }

        entries.sort_by(|a, b| a.vmid.cmp(&b.vmid));
        for entry in entries {
            print!("{}: {}", entry.vmid, entry.status);
            if let Some(name) = &entry.name {
                print!(" ({name})");
            }
            print!("{}", FormatTagList(entry.tags().collect()));
            println!();
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
/// Query the configuration of a VM.
async fn get_qemu_config(
    remote: String,
    node: Option<String>,
    vmid: u32,
    state: Option<pdm_api_types::ConfigurationState>,
    snapshot: Option<String>,
) -> Result<(), Error> {
    let config = client()?
        .pve_qemu_config(
            &remote,
            node.as_deref(),
            vmid,
            state.unwrap_or_default(),
            snapshot.as_deref(),
        )
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
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA },
        }
    }
)]
/// Start a VM.
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
/// Shutdown a VM.
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
/// Stop a VM abruptly.
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
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA },
            target: { schema: REMOTE_ID_SCHEMA },
            bwlimit: {
                description: "Override I/O bandwidth limit (in KiB/s).",
                optional: true,
            },
            online: {
                type: bool,
                description: "Perform an online migration if the vm is running.",
                optional: true,
            },
            force: {
                type: bool,
                description: "Perform an online migration if the vm is running.",
                optional: true,
            },
            "with-local-disks": {
                description: "Enable live storage migration for local disks.",
                optional: true,
            },
            "migration-network": {
                description: "CIDR of the (sub) network that is used for migration.",
                type: String,
                format: &CIDR_FORMAT,
                optional: true,
            },
            "migration-type": {
                type: StartQemuMigrationType,
                optional: true,
            },
            "map-storage": {
                type: Array,
                optional: true,
                description: "Mapping of source storages to ones on the target cluster.",
                items: {
                    type: String,
                    description: "Map a source storage to a target storage.",
                    type_text: "FROM:TO,...",
                },
            },
        }
    }
)]
#[allow(clippy::too_many_arguments)]
/// Migrate a VM to a different node of the same cluster.
async fn migrate_qemu(
    remote: String,
    node: Option<String>,
    vmid: u32,
    target: String,
    bwlimit: Option<u64>,
    online: Option<bool>,
    force: Option<bool>,
    with_local_disks: Option<bool>,
    migration_network: Option<String>,
    migration_type: Option<StartQemuMigrationType>,
    map_storage: Option<Vec<String>>,
) -> Result<(), Error> {
    let mut params = pdm_client::MigrateQemu::new();
    if let Some(bwlimit) = bwlimit {
        params = params.bwlimit(bwlimit);
    }
    if let Some(online) = online {
        params = params.online(online);
    }
    if let Some(force) = force {
        params = params.force(force);
    }
    if let Some(with_local_disks) = with_local_disks {
        params = params.with_local_disks(with_local_disks);
    }
    if let Some(migration_network) = migration_network {
        params = params.migration_network(migration_network);
    }
    if let Some(migration_type) = migration_type {
        params = params.migration_type(migration_type);
    }
    for mapping in map_storage.into_iter().flatten() {
        let pos = mapping
            .find(':')
            .ok_or_else(|| format_err!("missing ':' in storage mapping {mapping:?}"))?;
        params = params.map_storage(&mapping[..pos], &mapping[(pos + 1)..]);
    }

    let client = client()?;
    let upid = client
        .pve_qemu_migrate(&remote, node.as_deref(), vmid, target, params)
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
            target: { schema: REMOTE_ID_SCHEMA },
            "target-vmid": {
                optional: true,
                schema: VMID_SCHEMA,
            },
            delete: {
                description: "Delete the original VM and related data after successful migration.",
                optional: true,
            },
            online: {
                type: bool,
                description: "Perform an online migration if the vm is running.",
                optional: true,
            },
            "map-storage": {
                type: Array,
                description: "Mapping of source storages to ones on the target cluster.",
                items: {
                    type: String,
                    description: "Map a source storage to a target storage.",
                    type_text: "FROM:TO,...",
                },
            },
            "map-bridge": {
                type: Array,
                description: "Mapping of source network bridges to ones on the target cluster.",
                items: {
                    type: String,
                    description: "Map a source network bridge to a target bridge.",
                    type_text: "FROM:TO,...",
                },
            },
            bwlimit: {
                description: "Override I/O bandwidth limit (in KiB/s).",
                optional: true,
            },
        }
    }
)]
#[allow(clippy::too_many_arguments)]
/// Migrate a VM to a different cluster.
async fn remote_migrate_qemu(
    remote: String,
    node: Option<String>,
    vmid: u32,
    target: String,
    target_vmid: Option<u32>,
    online: Option<bool>,
    delete: Option<bool>,
    map_storage: Vec<String>,
    map_bridge: Vec<String>,
    bwlimit: Option<u64>,
) -> Result<(), Error> {
    let mut params = pdm_client::RemoteMigrateQemu::new();
    if let Some(online) = online {
        params = params.online(online);
    }
    if let Some(delete) = delete {
        params = params.delete_source(delete);
    }
    if let Some(bwlimit) = bwlimit {
        params = params.bwlimit(bwlimit);
    }
    if let Some(target_vmid) = target_vmid {
        params = params.target_vmid(target_vmid);
    }
    for mapping in map_storage {
        let pos = mapping
            .find(':')
            .ok_or_else(|| format_err!("missing ':' in storage mapping {mapping:?}"))?;
        params = params.map_storage(&mapping[..pos], &mapping[(pos + 1)..]);
    }
    for mapping in map_bridge {
        let pos = mapping
            .find(':')
            .ok_or_else(|| format_err!("missing ':' in bridge mapping {mapping:?}"))?;
        params = params.map_bridge(&mapping[..pos], &mapping[(pos + 1)..]);
    }

    let client = client()?;
    let upid = client
        .pve_qemu_remote_migrate(&remote, node.as_deref(), vmid, target, None, params)
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
            vmid: {
                schema: VMID_SCHEMA,
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
/// Return a VM's metric data.
async fn get_qemu_rrd_data(
    remote: String,
    vmid: u32,
    mode: RrdMode,
    timeframe: RrdTimeframe,
) -> Result<(), Error> {
    let config = client()?
        .pve_qemu_rrddata(&remote, vmid, mode, timeframe)
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
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
        }
    }
)]
/// List the LXC containers of a cluster.
async fn list_lxc(remote: String, node: Option<String>) -> Result<(), Error> {
    let mut entries = client()?.pve_list_lxc(&remote, node.as_deref()).await?;

    let output_format = env().format_args.output_format;
    if output_format == OutputFormat::Text {
        if entries.is_empty() {
            println!("No containers available");
            return Ok(());
        }

        entries.sort_by(|a, b| a.vmid.cmp(&b.vmid));
        for entry in entries {
            print!("{}: {}", entry.vmid, entry.status);
            if let Some(name) = &entry.name {
                print!(" ({name})");
            }
            print!("{}", FormatTagList(entry.tags().collect()));
            println!();
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
/// Query the configuration of a container.
async fn get_lxc_config(
    remote: String,
    node: Option<String>,
    vmid: u32,
    state: Option<pdm_api_types::ConfigurationState>,
    snapshot: Option<String>,
) -> Result<(), Error> {
    let config = client()?
        .pve_lxc_config(
            &remote,
            node.as_deref(),
            vmid,
            state.unwrap_or_default(),
            snapshot.as_deref(),
        )
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
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA },
        }
    }
)]
/// Start a container.
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
/// Shutdown a container.
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
/// Stop a container abruptly.
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
            remote: { schema: REMOTE_ID_SCHEMA },
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
            vmid: { schema: VMID_SCHEMA },
            target: { schema: REMOTE_ID_SCHEMA },
            bwlimit: {
                description: "Override I/O bandwidth limit (in KiB/s).",
                optional: true,
            },
            online: {
                type: bool,
                description: "Perform an online migration if the vm is running.",
                optional: true,
            },
            restart: {
                description: "Perform a restart-migration.",
                optional: true,
                default: false,
            },
            "map-storage": {
                type: Array,
                optional: true,
                description: "Mapping of source storages to ones on the target cluster.",
                items: {
                    type: String,
                    description: "Map a source storage to a target storage.",
                    type_text: "FROM:TO,...",
                },
            },
            timeout: {
                description: "Add a shutdown timeout for the restart-migration.",
                optional: true,
            },
        }
    }
)]
#[allow(clippy::too_many_arguments)]
/// Migrate a container to a different node in the same cluster.
async fn migrate_lxc(
    remote: String,
    node: Option<String>,
    vmid: u32,
    target: String,
    bwlimit: Option<u64>,
    online: Option<bool>,
    restart: bool,
    map_storage: Option<Vec<String>>,
    timeout: Option<u64>,
) -> Result<(), Error> {
    let mut params = pdm_client::MigrateLxc::new();
    if let Some(bwlimit) = bwlimit {
        params = params.bwlimit(bwlimit);
    }
    if let Some(online) = online {
        params = params.online(online);
    }
    if restart {
        params = params.restart(true, timeout.map(Duration::from_secs));
    }
    for mapping in map_storage.into_iter().flatten() {
        let pos = mapping
            .find(':')
            .ok_or_else(|| format_err!("missing ':' in storage mapping {mapping:?}"))?;
        params = params.map_storage(&mapping[..pos], &mapping[(pos + 1)..]);
    }

    let client = client()?;
    let upid = client
        .pve_lxc_migrate(&remote, node.as_deref(), vmid, target, params)
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
            target: { schema: REMOTE_ID_SCHEMA },
            delete: {
                description: "Delete the original VM and related data after successful migration.",
                optional: true,
            },
            online: {
                type: bool,
                description: "Perform an online migration if the vm is running.",
                optional: true,
            },
            "map-storage": {
                type: Array,
                description: "Mapping of source storages to ones on the target cluster.",
                items: {
                    type: String,
                    description: "Map a source storage to a target storage.",
                    type_text: "FROM:TO,...",
                },
            },
            "map-bridge": {
                type: Array,
                description: "Mapping of source network bridges to ones on the target cluster.",
                items: {
                    type: String,
                    description: "Map a source network bridge to a target bridge.",
                    type_text: "FROM:TO,...",
                },
            },
            bwlimit: {
                description: "Override I/O bandwidth limit (in KiB/s).",
                optional: true,
            },
            restart: {
                description: "Perform a restart-migration.",
                optional: true,
                default: false,
            },
            timeout: {
                description: "Add a shutdown timeout for the restart-migration.",
                optional: true,
            },
        }
    }
)]
#[allow(clippy::too_many_arguments)]
/// Migrate a container to a different cluster.
async fn remote_migrate_lxc(
    remote: String,
    node: Option<String>,
    vmid: u32,
    target: String,
    online: Option<bool>,
    delete: Option<bool>,
    map_storage: Vec<String>,
    map_bridge: Vec<String>,
    bwlimit: Option<u64>,
    restart: bool,
    timeout: Option<u64>,
) -> Result<(), Error> {
    let mut params = pdm_client::RemoteMigrateLxc::new();
    if let Some(online) = online {
        params = params.online(online);
    }
    if let Some(delete) = delete {
        params = params.delete_source(delete);
    }
    if let Some(bwlimit) = bwlimit {
        params = params.bwlimit(bwlimit);
    }
    if restart {
        params = params.restart(true, timeout.map(Duration::from_secs));
    }
    for mapping in map_storage {
        let pos = mapping
            .find(':')
            .ok_or_else(|| format_err!("missing ':' in storage mapping {mapping:?}"))?;
        params = params.map_storage(&mapping[..pos], &mapping[(pos + 1)..]);
    }
    for mapping in map_bridge {
        let pos = mapping
            .find(':')
            .ok_or_else(|| format_err!("missing ':' in bridge mapping {mapping:?}"))?;
        params = params.map_bridge(&mapping[..pos], &mapping[(pos + 1)..]);
    }

    let client = client()?;
    let upid = client
        .pve_lxc_remote_migrate(&remote, node.as_deref(), vmid, target, None, params)
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
            vmid: {
                schema: VMID_SCHEMA,
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
/// Return a CT's metric data.
async fn get_lxc_rrd_data(
    remote: String,
    vmid: u32,
    mode: RrdMode,
    timeframe: RrdTimeframe,
) -> Result<(), Error> {
    let config = client()?
        .pve_lxc_rrddata(&remote, vmid, mode, timeframe)
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
            node: {
                schema: NODE_SCHEMA,
                optional: true,
            },
        }
    }
)]
/// List the tasks of a cluster.
async fn list_tasks(remote: String, node: Option<String>) -> Result<(), Error> {
    const TASK_LIST_SCHEMA: Schema =
        ArraySchema::new("task list", &pve_api_types::ListTasksResponse::API_SCHEMA).schema();

    let data = client()?.pve_list_tasks(&remote, node.as_deref()).await?;

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
    let data = client()?.pve_task_status(&upid).await?;

    format_and_print_result_full(
        &mut serde_json::to_value(data)?,
        &ReturnType {
            optional: false,
            schema: &pve_api_types::TaskStatus::API_SCHEMA,
        },
        &env().format_args.output_format.to_string(),
        &Default::default(),
    );
    Ok(())
}
