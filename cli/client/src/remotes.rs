use anyhow::Error;

use proxmox_router::cli::{
    format_and_print_result, format_and_print_result_full, CliCommand, CliCommandMap,
    CommandLineInterface, OutputFormat,
};
use proxmox_schema::property_string::PropertyString;
use proxmox_schema::{api, property_string, ApiType, ReturnType, Schema};

use pdm_api_types::remotes::{
    NodeUrl, Remote, RemoteType, RemoteUpdater, TlsProbeOutcome, REMOTE_ID_SCHEMA,
};
use pdm_api_types::CERT_FINGERPRINT_SHA256_SCHEMA;

use crate::{client, env};

pub fn cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert("list", CliCommand::new(&API_METHOD_LIST_REMOTES))
        .insert(
            "add",
            CliCommand::new(&API_METHOD_ADD_REMOTE).arg_param(&["type", "id"]),
        )
        .insert(
            "delete",
            CliCommand::new(&API_METHOD_DELETE_REMOTE).arg_param(&["id"]),
        )
        .insert(
            "update",
            CliCommand::new(&API_METHOD_UPDATE_REMOTE).arg_param(&["id"]),
        )
        .insert(
            "version",
            CliCommand::new(&API_METHOD_REMOTE_VERSION).arg_param(&["id"]),
        )
        .insert(
            "probe-certificate",
            CliCommand::new(&API_METHOD_PROBE_CERTIFICATE).arg_param(&["id", "node"]),
        )
        .insert(
            "set-fingerprint",
            CliCommand::new(&API_METHOD_SET_FINGERPRINT).arg_param(&["id", "node"]),
        )
        .into()
}

#[api]
/// List all the remotes this instance is managing.
async fn list_remotes() -> Result<(), Error> {
    let entries = client()?.list_remotes().await?;

    let output_format = env().format_args.output_format;
    if output_format == OutputFormat::Text {
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
        format_and_print_result(&entries, &output_format.to_string());
    }
    Ok(())
}

const CREATE_TOKEN_SCHEMA: Schema = pdm_api_types::PROXMOX_TOKEN_NAME_SCHEMA
    .unwrap_string_schema_cloned()
    .description("If given, create this token on the remote and use it.")
    .schema();

// FIXME: Support `OneOf` in schema so we can just use the `Remote` enum api schema here as input.
#[api(
    input: {
        properties: {
            entry: {
                flatten: true,
                type: Remote,
            },
            "create-token": {
                optional: true,
                schema: CREATE_TOKEN_SCHEMA,
            },
        }
    }
)]
/// Add a new remote.
async fn add_remote(entry: Remote, create_token: Option<String>) -> Result<(), Error> {
    client()?
        .add_remote(&entry, create_token.as_deref())
        .await?;
    Ok(())
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
            delete: {
                description: "List of properties to clear, e.g. 'group' or 'web-url'.",
                type: Array,
                optional: true,
                items: {
                    type: String,
                    description: "Property name.",
                },
            },
        }
    }
)]
/// Update a remote.
async fn update_remote(
    id: String,
    updater: RemoteUpdater,
    delete: Option<Vec<String>>,
) -> Result<(), Error> {
    client()?
        .update_remote(&id, &updater, delete.as_deref().unwrap_or(&[]))
        .await?;
    Ok(())
}

#[api(
    input: {
        properties: {
            id: { schema: REMOTE_ID_SCHEMA },
            "delete-token": {
                optional: true,
                default: true,
                description: "If set to false, token deletion on the remote is skipped."
            }
        }
    }
)]
/// Delete a remote.
async fn delete_remote(id: String, delete_token: bool) -> Result<(), Error> {
    client()?.delete_remote(&id, Some(delete_token)).await?;
    Ok(())
}

#[api(
    input: {
        properties: {
            id: { schema: REMOTE_ID_SCHEMA },
            node: {
                type: String,
                description: "Hostname of the configured node to probe.",
            },
        }
    }
)]
/// Re-probe a configured node's TLS certificate, ignoring the pinned fingerprint.
///
/// Useful to detect a rotated certificate. Apply the new fingerprint with
/// `remote set-fingerprint <id> <node> <fingerprint>`, or clear the pin by omitting it.
async fn probe_certificate(id: String, node: String) -> Result<(), Error> {
    let outcome = client()?.remote_probe_certificate(&id, &node).await?;
    match outcome {
        TlsProbeOutcome::TrustedCertificate => {
            println!("Certificate is trusted (validates against the system certificate store).");
        }
        TlsProbeOutcome::UntrustedCertificate(info) => {
            let date = |epoch: Option<i64>| {
                epoch
                    .and_then(|e| proxmox_time::epoch_to_rfc3339_utc(e).ok())
                    .unwrap_or_else(|| "-".to_string())
            };
            println!("Presented certificate (not trusted by the system store):");
            println!(
                "    fingerprint: {}",
                info.fingerprint.as_deref().unwrap_or("-")
            );
            println!("    subject:     {}", info.subject);
            println!("    issuer:      {}", info.issuer);
            println!("    valid since: {}", date(info.notbefore));
            println!("    expires:     {}", date(info.notafter));
        }
    }
    Ok(())
}

#[api(
    input: {
        properties: {
            id: { schema: REMOTE_ID_SCHEMA },
            node: {
                type: String,
                description: "Hostname of the configured node whose fingerprint to set.",
            },
            fingerprint: {
                schema: CERT_FINGERPRINT_SHA256_SCHEMA,
                optional: true,
            },
        }
    }
)]
/// Set or clear a node's stored TLS certificate fingerprint.
///
/// Omit the fingerprint to clear the pin and rely on the system trust store. Run
/// `remote probe-certificate` first to read the fingerprint the node currently presents.
async fn set_fingerprint(
    id: String,
    node: String,
    fingerprint: Option<String>,
) -> Result<(), Error> {
    let client = client()?;
    let remote = client
        .list_remotes()
        .await?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| anyhow::format_err!("no such remote '{id}'"))?;

    let mut found = false;
    let nodes: Vec<PropertyString<NodeUrl>> = remote
        .nodes
        .iter()
        .map(|ps| {
            let mut url: NodeUrl = (**ps).clone();
            if url.hostname == node {
                url.fingerprint = fingerprint.clone();
                found = true;
            }
            PropertyString::new(url)
        })
        .collect();
    if !found {
        anyhow::bail!("remote '{id}' has no node '{node}'");
    }

    let updater = RemoteUpdater {
        nodes: Some(nodes),
        ..Default::default()
    };
    client.update_remote(&id, &updater, &[]).await?;
    Ok(())
}

#[api(
    input: {
        properties: {
            id: { schema: REMOTE_ID_SCHEMA },
        }
    }
)]
/// Show a remote's version.
async fn remote_version(id: String) -> Result<(), Error> {
    let data = client()?.remote_version(&id).await?;
    format_and_print_result_full(
        &mut serde_json::to_value(data)?,
        &ReturnType::new(false, &pve_api_types::VersionResponse::API_SCHEMA),
        &env().format_args.output_format.to_string(),
        &Default::default(),
    );
    Ok(())
}
