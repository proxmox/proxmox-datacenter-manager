use serde_json::{json, Value};

use proxmox_router::cli::{
    default_table_format_options, format_and_print_result_full, get_output_format, run_cli_command,
    CliCommand, CliCommandMap, CliEnvironment, ColumnConfig, OUTPUT_FORMAT,
};

use proxmox_schema::api;

mod remotes;

fn main() {
    //pbs_tools::setup_libc_malloc_opts(); // TODO: move from PBS to proxmox-sys and uncomment

    let api_user = pdm_config::api_user().expect("cannot get api user");
    let priv_user = pdm_config::priv_user().expect("cannot get privileged user");
    proxmox_product_config::init(api_user, priv_user);

    proxmox_log::Logger::from_env("PDM_LOG", proxmox_log::LevelFilter::INFO)
        .stderr()
        .init()
        .expect("failed to set up logger");

    server::context::init().expect("could not set up server context");

    let cmd_def = CliCommandMap::new()
        .insert("remote", remotes::cli())
        .insert("versions", CliCommand::new(&API_METHOD_GET_VERSIONS));

    let rpcenv = CliEnvironment::new();
    run_cli_command(
        cmd_def,
        rpcenv,
        Some(|future| proxmox_async::runtime::main(future)),
    );
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
