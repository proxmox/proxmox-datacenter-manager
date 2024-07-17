use anyhow::Error;
use serde_json::Value;

use proxmox_router::cli::{
    get_output_format, CliCommand, CliCommandMap, CommandLineInterface, OUTPUT_FORMAT,
};
use proxmox_schema::api;

use pdm_api_types::ACL_PATH_SCHEMA;

use crate::client;

pub fn cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert("list", CliCommand::new(&API_METHOD_LIST_ACLS))
        .into()
}

#[api(
    input: {
        properties: {
            "output-format": {
                schema: OUTPUT_FORMAT,
                optional: true,
            },
            path: {
                schema: ACL_PATH_SCHEMA,
                optional: true,
            },
            exact: {
                description: "If set, returns only ACL for the exact path.",
                type: bool,
                optional: true,
                default: false,
            },
        }
    }
)]
/// List all users or show a single user's information.
async fn list_acls(path: Option<String>, exact: bool, param: Value) -> Result<(), Error> {
    let _output_format = get_output_format(&param);

    let client = client()?;

    let (entries, _digest) = client.read_acl(path.as_deref(), exact).await?;

    for entry in entries {
        println!(
            "{path}: {ugid} {ugid_type} {propagate} {roleid}",
            path = entry.path,
            ugid = entry.ugid,
            ugid_type = entry.ugid_type,
            propagate = entry.propagate,
            roleid = entry.roleid,
        );
    }

    Ok(())
}
