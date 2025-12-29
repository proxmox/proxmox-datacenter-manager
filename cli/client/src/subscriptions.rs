use anyhow::Error;

use proxmox_router::cli::{
    format_and_print_result, CliCommand, CommandLineInterface, OutputFormat,
};
use proxmox_schema::api;

use pdm_api_types::subscription::RemoteSubscriptionState;
use pdm_api_types::VIEW_ID_SCHEMA;

use crate::env::emoji;
use crate::{client, env};

pub fn cli() -> CommandLineInterface {
    CliCommand::new(&API_METHOD_GET_SUBSCRIPTION_STATUS).into()
}

#[api(
    input: {
        properties: {
            "max-age": {
                description: "Maximum age (in seconds) of cached remote subscription state.",
                // long default to not query it too often
                default: 24*60*60,
                optional: true,
            },
            verbose: {
                type: bool,
                optional: true,
                default: false,
                description: "If true, includes subscription information per node (with enough privileges)",
            },
            view: {
                schema: VIEW_ID_SCHEMA,
                optional: true,
            },
        },
    }
)]
/// List all the remotes this instance is managing.
async fn get_subscription_status(
    max_age: Option<u64>,
    verbose: Option<bool>,
    view: Option<String>,
) -> Result<(), Error> {
    let mut subscriptions = client()?
        .get_subscription_status(max_age, verbose, view.as_deref())
        .await?;
    let output_format = env().format_args.output_format;
    if output_format == OutputFormat::Text {
        if subscriptions.is_empty() {
            println!("No subscriptions.");
            return Ok(());
        }

        subscriptions.sort_by(|a, b| a.remote.cmp(&b.remote));
        let remote_width = subscriptions
            .iter()
            .map(|sub| sub.remote.len())
            .max()
            .unwrap_or(5);
        for entry in subscriptions {
            let mut symbol = "⚠️ ";
            let state = match entry.state {
                RemoteSubscriptionState::None => "at least one node with no subscription",
                RemoteSubscriptionState::Unknown => "status could not be determined",
                RemoteSubscriptionState::Mixed => "different subscription levels found",
                RemoteSubscriptionState::Active => {
                    symbol = "✅️ ";
                    "valid"
                }
            };
            let symbol = emoji(symbol);
            println!(
                "{symbol}{remote:width$} {state}",
                remote = format!("{}:", entry.remote),
                width = remote_width + 1,
            );

            if let Some(err) = entry.error {
                println!("    Errors querying the remote: {err}");
            }

            let Some(node_status) = entry.node_status else {
                continue;
            };

            let mut node_status = node_status.iter().collect::<Vec<_>>();

            node_status.sort_by(|a, b| a.0.cmp(b.0));
            for (node, info) in node_status {
                let Some(info) = info else {
                    println!("    {node}");
                    continue;
                };

                print!("    {node}: {} {}", info.status, info.level);
                if let Some(sockets) = info.sockets {
                    print!(" [{sockets} sockets]");
                }
                println!();
            }
        }
    } else {
        format_and_print_result(&subscriptions, &output_format.to_string());
    }
    Ok(())
}
