use core::matches;

use anyhow::{Context, Error};
use serde_json::{json, Value};

use proxmox_router::cli::{
    default_table_format_options, format_and_print_result_full, get_output_format,
    run_async_cli_command, CliCommand, CliCommandMap, CliEnvironment, ColumnConfig, OUTPUT_FORMAT,
};
use proxmox_router::RpcEnvironment;
use proxmox_schema::api;
use proxmox_sys::fs::CreateOptions;

mod acme;
mod remotes;
mod support_status;

async fn run() -> Result<(), Error> {
    let api_user = pdm_config::api_user().context("could not get api user")?;
    let priv_user = pdm_config::priv_user().context("could not get privileged user")?;

    proxmox_product_config::init(api_user.clone(), priv_user);
    proxmox_access_control::init::init(
        &pdm_api_types::AccessControlConfig,
        pdm_buildcfg::configdir!("/access"),
    )
    .context("failed to setup access control config")?;
    proxmox_acme_api::init(pdm_buildcfg::configdir!("/acme"), false)
        .context("failed to initialize acme config")?;

    proxmox_log::Logger::from_env("PDM_LOG", proxmox_log::LevelFilter::INFO)
        .tasklog_pbs()
        .stderr()
        .init()
        .context("failed to set up logger")?;

    server::context::init().context("could not set up server context")?;

    let cmd_def = CliCommandMap::new()
        .insert("acme", acme::acme_mgmt_cli())
        .insert("remote", remotes::cli())
        .insert(
            "report",
            CliCommand::new(&API_METHOD_GENERATE_SYSTEM_REPORT),
        )
        .insert("support-status", support_status::cli())
        .insert("versions", CliCommand::new(&API_METHOD_GET_VERSIONS));

    let args: Vec<String> = std::env::args().collect();
    let avoid_init = matches!(
        args.get(1).map(String::as_str),
        Some("bashcomplete") | Some("printdoc")
    );

    if !avoid_init {
        let file_opts = CreateOptions::new().owner(api_user.uid).group(api_user.gid);
        proxmox_rest_server::init_worker_tasks(pdm_buildcfg::PDM_LOG_DIR_M!().into(), file_opts)
            .context("failed to initialize worker tasks")?;

        let mut command_sock = proxmox_daemon::command_socket::CommandSocket::new(api_user.gid);
        proxmox_rest_server::register_task_control_commands(&mut command_sock)
            .context("failed to register task control commands")?;
        command_sock
            .spawn(proxmox_rest_server::last_worker_future())
            .context("failed to activate the socket")?;
    }

    let mut rpcenv = CliEnvironment::new();
    rpcenv.set_auth_id(Some("root@pam".into()));

    run_async_cli_command(cmd_def, rpcenv).await;

    Ok(())
}

fn main() -> Result<(), Error> {
    //pbs_tools::setup_libc_malloc_opts(); // TODO: move from PBS to proxmox-sys and uncomment
    proxmox_async::runtime::main(run())
}

#[api(
    input: {
        properties: {
            verbose: {
                type: Boolean,
                optional: true,
                default: false,
                description: "Output verbose package information. It is ignored if output-format is specified.",
            },
            "output-format": {
                schema: OUTPUT_FORMAT,
                optional: true,
            }
        }
    }
)]
/// List package versions for important Proxmox Datacenter Manager packages.
async fn get_versions(verbose: bool, param: Value) -> Result<Value, anyhow::Error> {
    let output_format = get_output_format(&param);

    let packages = server::api::nodes::apt::get_versions()?;
    let mut packages = json!(if verbose {
        &packages[..]
    } else {
        &packages[1..2]
    });

    let options = default_table_format_options()
        .disable_sort()
        .noborder(true) // just not helpful for version info which gets copy pasted often
        .column(ColumnConfig::new("Package"))
        .column(ColumnConfig::new("Version"))
        .column(ColumnConfig::new("ExtraInfo").header("Extra Info"));
    let return_type = &server::api::nodes::apt::API_METHOD_GET_VERSIONS.returns;

    format_and_print_result_full(&mut packages, return_type, &output_format, &options);

    Ok(Value::Null)
}

#[api]
/// Generate the system report.
async fn generate_system_report() -> Result<(), anyhow::Error> {
    let report = server::api::nodes::report::generate_system_report()?;
    print!("{report}");

    Ok(())
}
