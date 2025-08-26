use anyhow::Error;
use pdm_api_types::remotes::REMOTE_ID_SCHEMA;
use proxmox_router::cli::{
    format_and_print_result, CliCommand, CliCommandMap, CommandLineInterface, OutputFormat,
};
use proxmox_schema::api;

use crate::{client, env};

pub fn cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert(
            "trigger",
            CliCommand::new(&API_METHOD_TRIGGER_METRIC_COLLECTION),
        )
        .insert(
            "status",
            CliCommand::new(&API_METHOD_METRIC_COLLECTION_STATUS),
        )
        .into()
}

#[api(
    input: {
        properties: {
            remote: {
                schema: REMOTE_ID_SCHEMA,
                optional: true,
            },
        }
    }
)]
/// Trigger metric collection. If a remote is passed, only this remote will be collected, otherwise
/// all.
async fn trigger_metric_collection(remote: Option<String>) -> Result<(), Error> {
    client()?
        .trigger_metric_collection(remote.as_deref())
        .await?;
    Ok(())
}

#[api]
/// Show metric collection status.
async fn metric_collection_status() -> Result<(), Error> {
    let result = client()?.get_metric_collection_status().await?;

    let output_format = env().format_args.output_format;
    if output_format == OutputFormat::Text {
        for remote_status in result {
            let timestamp = if let Some(last_collection) = remote_status.last_collection {
                proxmox_time::strftime_local("%a, %d %b %Y %T %z", last_collection)?
            } else {
                "never".into()
            };

            let status = if let Some(err) = &remote_status.error {
                err
            } else {
                "ok"
            };

            println!("{}: {status}", remote_status.remote);
            println!("    last successful: {timestamp}");
            println!();
        }
    } else {
        format_and_print_result(&result, &output_format.to_string());
    }
    Ok(())
}
