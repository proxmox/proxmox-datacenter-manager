use serde_json::{json, Value};

use proxmox_router::cli::{
    format_and_print_result_full, get_output_format, CliCommand, CliCommandMap,
    CommandLineInterface,
};
use proxmox_router::{ApiHandler, RpcEnvironment};
use proxmox_schema::api;

pub fn cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert("get", CliCommand::new(&API_METHOD_SUPPORT_ELIGIBILITY))
        .insert(
            "update",
            CliCommand::new(&API_METHOD_UPDATE_SUPPORT_ELIGIBILITY),
        )
        .into()
}

#[api]
/// Get the support eligibility information.
async fn support_eligibility(
    param: Value,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<(), anyhow::Error> {
    let info = &server::api::nodes::subscription::API_METHOD_GET_SUBSCRIPTION;

    let mut data = match info.handler {
        ApiHandler::Async(handler) => (handler)(json!({}), info, rpcenv).await?,
        _ => unreachable!(),
    };

    let output_format = get_output_format(&param);

    format_and_print_result_full(
        &mut data,
        &info.returns,
        &output_format,
        &Default::default(),
    );
    Ok(())
}

#[api]
/// Update the support eligibility information.
async fn update_support_eligibility(
    param: Value,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<(), anyhow::Error> {
    let info = &server::api::nodes::subscription::API_METHOD_CHECK_SUBSCRIPTION;

    let mut data = match info.handler {
        ApiHandler::Async(handler) => (handler)(json!({}), info, rpcenv).await?,
        _ => unreachable!(),
    };

    let output_format = get_output_format(&param);

    format_and_print_result_full(
        &mut data,
        &info.returns,
        &output_format,
        &Default::default(),
    );
    Ok(())
}
