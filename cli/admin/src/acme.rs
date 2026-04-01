use std::io::Write;

use anyhow::{bail, Error};
use serde_json::Value;

use pdm_api_types::acme::AcmeRegistrationParams;
use proxmox_acme::async_client::AcmeClient;
use proxmox_acme_api::{completion::*, AcmeAccountName, DnsPluginCore, KNOWN_ACME_DIRECTORIES};
use proxmox_rest_server::wait_for_local_worker;
use proxmox_router::{cli::*, ApiHandler, RpcEnvironment};
use proxmox_schema::api;
use proxmox_sys::fs::file_get_contents;

use server::api as dc_api;

pub fn acme_mgmt_cli() -> CommandLineInterface {
    let cmd_def = CliCommandMap::new()
        .insert("account", account_cli())
        .insert("plugin", plugin_cli())
        .insert("certificate", cert_cli());

    cmd_def.into()
}

#[api(
    input: {
        properties: {
                "output-format": {
                    schema: OUTPUT_FORMAT,
                    optional: true,
                }
            }
    }
)]
/// List ACME accounts.
fn list_accounts(param: Value, rpcenv: &mut dyn RpcEnvironment) -> Result<(), Error> {
    let output_format = get_output_format(&param);
    let info = &dc_api::config::acme::API_METHOD_LIST_ACCOUNTS;
    let mut data = match info.handler {
        ApiHandler::Sync(handler) => (handler)(param, info, rpcenv)?,
        _ => unreachable!(),
    };

    let options = default_table_format_options();
    format_and_print_result_full(&mut data, &info.returns, &output_format, &options);

    Ok(())
}

#[api(
    input: {
        properties: {
            name: { type:  AcmeAccountName },
            "output-format": {
                schema: OUTPUT_FORMAT,
                optional: true,
            },
        }
    }
)]
/// Show ACME account information.
async fn get_account(param: Value, rpcenv: &mut dyn RpcEnvironment) -> Result<(), Error> {
    let output_format = get_output_format(&param);

    let info = &dc_api::config::acme::API_METHOD_GET_ACCOUNT;
    let mut data = match info.handler {
        ApiHandler::Async(handler) => (handler)(param, info, rpcenv).await?,
        _ => unreachable!(),
    };

    let options = default_table_format_options()
        .column(
            ColumnConfig::new("account")
                .renderer(|value, _record| Ok(serde_json::to_string_pretty(value)?)),
        )
        .column(ColumnConfig::new("directory"))
        .column(ColumnConfig::new("location"))
        .column(ColumnConfig::new("tos"));
    format_and_print_result_full(&mut data, &info.returns, &output_format, &options);

    Ok(())
}

fn read_input(prompt: &str) -> Result<String, Error> {
    print!("{}: ", prompt);
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    Ok(input)
}

#[api(
    input: {
        properties: {
            name: { type: AcmeAccountName },
            contact: {
                description: "List of email addresses.",
                type: String,
            },
            directory: {
                type: String,
                description: "The ACME Directory.",
                optional: true,
            },
        }
    }
)]
///Register an ACME account.
async fn register_account(
    name: AcmeAccountName,
    contact: String,
    directory: Option<String>,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<(), Error> {
    let (directory_url, custom_directory) = match directory {
        Some(directory) => (directory, true),
        None => {
            println!("Directory endpoints:");
            for (i, dir) in KNOWN_ACME_DIRECTORIES.iter().enumerate() {
                println!("{}) {}", i + 1, dir.url);
            }

            let known_acme_dir_len = KNOWN_ACME_DIRECTORIES.len();
            println!("{}) Custom", known_acme_dir_len + 1);
            let mut attempt = 0;
            loop {
                let mut input = read_input("Enter selection")?;
                match input.trim().parse::<usize>() {
                    Ok(n) if (1..=known_acme_dir_len).contains(&n) => {
                        break (KNOWN_ACME_DIRECTORIES[n - 1].url.to_string(), false);
                    }
                    Ok(n) if n == known_acme_dir_len + 1 => {
                        input.clear();
                        input = read_input("Enter custom directory URI")?;
                        break (input.trim().to_owned(), true);
                    }
                    _ => eprintln!("Invalid selection."),
                }

                attempt += 1;
                if attempt >= 3 {
                    bail!("Aborting.");
                }
            }
        }
    };

    println!("Attempting to fetch Terms of Service from {directory_url:?}");
    let mut client = AcmeClient::new(directory_url.clone());
    let directory = client.directory().await?;
    let tos_agreed = if let Some(tos_url) = directory.terms_of_service_url() {
        println!("Terms of Service: {tos_url}");
        let input = read_input("Do you agree to the above terms? [y|N]")?;
        input.trim().eq_ignore_ascii_case("y")
    } else {
        println!("No Terms of Service found, proceeding.");
        true
    };

    let mut eab_enabled = directory.external_account_binding_required();
    if !eab_enabled && custom_directory {
        let input = read_input("Do you want to use external account binding? [y|N]")?;
        eab_enabled = input.trim().eq_ignore_ascii_case("y");
    } else if eab_enabled {
        println!("The CA requires external account binding.");
    }

    let eab_creds = if eab_enabled {
        println!("You should have received a key id and a key from your CA.");
        let eab_kid = read_input("Enter EAB key id")?;
        let eab_hmac_key = read_input("Enter EAB key")?;
        Some((eab_kid.trim().to_owned(), eab_hmac_key.trim().to_owned()))
    } else {
        None
    };

    let tos_url = tos_agreed
        .then(|| directory.terms_of_service_url().map(str::to_owned))
        .flatten();

    let (eab_kid, eab_hmac_key) = eab_creds.unzip();
    let parameters = AcmeRegistrationParams {
        name: Some(name),
        contact: contact,
        tos_url: tos_url,
        directory: Some(directory_url),
        eab_kid: eab_kid,
        eab_hmac_key: eab_hmac_key,
    };
    let param = serde_json::to_value(parameters)?;

    let info = &dc_api::config::acme::API_METHOD_REGISTER_ACCOUNT;
    let result = match info.handler {
        ApiHandler::Sync(handler) => (handler)(param, info, rpcenv)?,
        _ => unreachable!(),
    };

    wait_for_local_worker(result.as_str().unwrap()).await?;

    Ok(())
}

#[api(
    input: {
       properties: {
           name: { type: AcmeAccountName },
           contact:  {
               description: "List of email addresses.",
               type: String,
               optional: true,
           }
       }
    }
)]
/// Update an ACME Account.
async fn update_account(param: Value, rpcenv: &mut dyn RpcEnvironment) -> Result<(), Error> {
    let info = &dc_api::config::acme::API_METHOD_UPDATE_ACCOUNT;
    let result = match info.handler {
        ApiHandler::Sync(handler) => (handler)(param, info, rpcenv)?,
        _ => unreachable!(),
    };

    wait_for_local_worker(result.as_str().unwrap()).await?;

    Ok(())
}

#[api(
    input: {
        properties: {
            name: { type: AcmeAccountName },
            force: {
                description: "Delete account data even if the server refuses to deactivate the account.",
                type: Boolean,
                optional: true,
                default: true,
            }
        }
    }
)]
/// Deactivate an ACME account.
async fn deactivate_account(param: Value, rpcenv: &mut dyn RpcEnvironment) -> Result<(), Error> {
    let info = &dc_api::config::acme::API_METHOD_DEACTIVATE_ACCOUNT;
    let result = match info.handler {
        ApiHandler::Sync(handler) => (handler)(param, info, rpcenv)?,
        _ => unreachable!(),
    };

    wait_for_local_worker(result.as_str().unwrap()).await?;

    Ok(())
}

fn account_cli() -> CommandLineInterface {
    let cmd_def = CliCommandMap::new()
        .insert("list", CliCommand::new(&API_METHOD_LIST_ACCOUNTS))
        .insert(
            "register",
            CliCommand::new(&API_METHOD_REGISTER_ACCOUNT).arg_param(&["name", "contact"]),
        )
        .insert(
            "deactivate",
            CliCommand::new(&API_METHOD_DEACTIVATE_ACCOUNT)
                .arg_param(&["name"])
                .completion_cb("name", complete_acme_account),
        )
        .insert(
            "info",
            CliCommand::new(&API_METHOD_GET_ACCOUNT)
                .arg_param(&["name"])
                .completion_cb("name", complete_acme_account),
        )
        .insert(
            "update",
            CliCommand::new(&API_METHOD_UPDATE_ACCOUNT)
                .arg_param(&["name", "contact"])
                .completion_cb("name", complete_acme_account),
        );

    cmd_def.into()
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
/// List ACME plugins.
fn list_plugins(param: Value, rpcenv: &mut dyn RpcEnvironment) -> Result<(), Error> {
    let output_format = get_output_format(&param);

    let info = &dc_api::config::acme::API_METHOD_LIST_PLUGINS;
    let mut data = match info.handler {
        ApiHandler::Sync(handler) => (handler)(param, info, rpcenv)?,
        _ => unreachable!(),
    };

    let options = default_table_format_options();
    format_and_print_result_full(&mut data, &info.returns, &output_format, &options);

    Ok(())
}

#[api(
    input: {
        properties: {
            id: {
                type: String,
                description: "Plugin ID",
            },
            "output-format": {
                schema: OUTPUT_FORMAT,
                optional: true,
            },
        }
    }
)]
/// Show ACME plugin information.
fn get_plugin(param: Value, rpcenv: &mut dyn RpcEnvironment) -> Result<(), Error> {
    let output_format = get_output_format(&param);

    let info = &dc_api::config::acme::API_METHOD_GET_PLUGIN;
    let mut data = match info.handler {
        ApiHandler::Sync(handler) => (handler)(param, info, rpcenv)?,
        _ => unreachable!(),
    };

    let options = default_table_format_options();
    format_and_print_result_full(&mut data, &info.returns, &output_format, &options);

    Ok(())
}

#[api(input: {
    properties: {
        type: {
            type: String,
            description: "The ACME challenge plugin type."
        },
        core: {
            type: DnsPluginCore,
            flatten: true,
        },
        data: {
            type: String,
            description: "File containing the plugin data."
        }
    }
})]
/// Add ACME plugin configuration.
fn add_plugin(r#type: String, core: DnsPluginCore, data: String) -> Result<(), Error> {
    let data = proxmox_base64::encode(file_get_contents(data)?);
    dc_api::config::acme::add_plugin(r#type, core, data)?;
    Ok(())
}

pub fn plugin_cli() -> CommandLineInterface {
    let cmd_def = CliCommandMap::new()
        .insert("list", CliCommand::new(&API_METHOD_LIST_PLUGINS))
        .insert(
            "config",
            CliCommand::new(&API_METHOD_GET_PLUGIN)
                .arg_param(&["id"])
                .completion_cb("id", complete_acme_plugin),
        )
        .insert(
            "add",
            CliCommand::new(&API_METHOD_ADD_PLUGIN)
                .arg_param(&["type", "id"])
                .completion_cb("api", complete_acme_api_challenge_type)
                .completion_cb("type", complete_acme_plugin_type),
        )
        .insert(
            "remove",
            CliCommand::new(&dc_api::config::acme::API_METHOD_DELETE_PLUGIN)
                .arg_param(&["id"])
                .completion_cb("id", complete_acme_plugin),
        )
        .insert(
            "set",
            CliCommand::new(&dc_api::config::acme::API_METHOD_UPDATE_PLUGIN)
                .arg_param(&["id"])
                .completion_cb("id", complete_acme_plugin),
        );

    cmd_def.into()
}

#[api(
    input: {
        properties: {
            force: {
                description: "Force renewal even if the certificate does not expire soon.",
                type: Boolean,
                optional: true,
                default: false,
            },
        },
    },
)]
/// Order a new ACME certificate.
async fn order_acme_cert(param: Value, rpcenv: &mut dyn RpcEnvironment) -> Result<(), Error> {
    if !param["force"].as_bool().unwrap_or(false)
        && !dc_api::nodes::certificates::cert_expires_soon()?
    {
        println!("Certificate does not expire within the next 30 days, not renewing.");
        return Ok(());
    }

    let info = &dc_api::nodes::certificates::API_METHOD_RENEW_ACME_CERT;
    let result = match info.handler {
        ApiHandler::Sync(handler) => (handler)(param, info, rpcenv)?,
        _ => unreachable!(),
    };

    wait_for_local_worker(result.as_str().unwrap()).await?;

    Ok(())
}

#[api]
/// Revoke ACME certificate.
async fn revoke_acme_cert(param: Value, rpcenv: &mut dyn RpcEnvironment) -> Result<(), Error> {
    let info = &dc_api::nodes::certificates::API_METHOD_REVOKE_ACME_CERT;
    let result = match info.handler {
        ApiHandler::Sync(handler) => (handler)(param, info, rpcenv)?,
        _ => unreachable!(),
    };

    wait_for_local_worker(result.as_str().unwrap()).await?;

    Ok(())
}

pub fn cert_cli() -> CommandLineInterface {
    let cmd_def = CliCommandMap::new()
        .insert("order", CliCommand::new(&API_METHOD_ORDER_ACME_CERT))
        .insert("revoke", CliCommand::new(&API_METHOD_REVOKE_ACME_CERT));

    cmd_def.into()
}
