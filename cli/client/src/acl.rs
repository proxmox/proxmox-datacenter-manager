use anyhow::{bail, Error};
use serde_json::Value;

use proxmox_config_digest::PROXMOX_CONFIG_DIGEST_SCHEMA;
use proxmox_router::cli::{
    get_output_format, CliCommand, CliCommandMap, CommandLineInterface, OUTPUT_FORMAT,
};
use proxmox_schema::api;

use pdm_api_types::{AclUgidType, Authid, Role, ACL_PATH_SCHEMA, ACL_PROPAGATE_SCHEMA};
use pdm_client::{AclRecipient, ConfigDigest};

use crate::client;

pub fn cli() -> CommandLineInterface {
    CliCommandMap::new()
        .insert("list", CliCommand::new(&API_METHOD_LIST_ACLS))
        .insert(
            "update",
            CliCommand::new(&API_METHOD_UPDATE_ACL).arg_param(&["path", "role"]),
        )
        .insert(
            "delete",
            CliCommand::new(&API_METHOD_DELETE_ACL).arg_param(&["path", "role"]),
        )
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
    let output_format = get_output_format(&param);

    let client = client()?;

    let (entries, _digest) = client.read_acl(path.as_deref(), exact).await?;

    match output_format.as_str() {
        "json" => println!("{}", serde_json::to_string(&entries)?),
        "json-pretty" => println!("{}", serde_json::to_string_pretty(&entries)?),
        "text" => {
            for entry in entries {
                println!(
                    "{ugid}: {path} {ugid_prefix}{ugid} {propagate} {roleid}",
                    path = entry.path,
                    ugid = entry.ugid,
                    propagate = entry.propagate,
                    roleid = entry.roleid,
                    ugid_prefix = match entry.ugid_type {
                        AclUgidType::User => "",
                        AclUgidType::Group => "@",
                    },
                );
            }
        }
        _ => bail!("unsupported output format {output_format:?}"),
    }

    Ok(())
}

#[api(
    input: {
        properties: {
            "auth-id": { type: Authid },
            /*
            group: {
                schema: PROXMOX_GROUP_ID_SCHEMA,
                optional: true,
            },
            */
            path: {
                schema: ACL_PATH_SCHEMA,
            },
            role: { type: Role },
            propagate: {
                schema: ACL_PROPAGATE_SCHEMA,
                optional: true,
            },
            digest: {
                schema: PROXMOX_CONFIG_DIGEST_SCHEMA,
                optional: true,
            },

        }
    }
)]
/// Add or update an ACL entry.
async fn update_acl(
    auth_id: Authid,
    path: String,
    role: String,
    propagate: Option<bool>,
    digest: Option<String>,
) -> Result<(), Error> {
    let propagate = propagate.unwrap_or(true);
    let digest = digest.map(ConfigDigest::from);

    client()?
        .update_acl(
            AclRecipient::Authid(&auth_id),
            &path,
            &role,
            propagate,
            digest,
        )
        .await?;
    Ok(())
}

#[api(
    input: {
        properties: {
            "auth-id": { type: Authid },
            /*
            group: {
                schema: PROXMOX_GROUP_ID_SCHEMA,
                optional: true,
            },
            */
            path: {
                schema: ACL_PATH_SCHEMA,
            },
            role: { type: Role },
            digest: {
                schema: PROXMOX_CONFIG_DIGEST_SCHEMA,
                optional: true,
            },

        }
    }
)]
/// Delete an ACL entry.
async fn delete_acl(
    auth_id: Authid,
    path: String,
    role: String,
    digest: Option<String>,
) -> Result<(), Error> {
    let digest = digest.map(ConfigDigest::from);

    client()?
        .delete_acl(AclRecipient::Authid(&auth_id), &path, &role, digest)
        .await?;
    Ok(())
}
