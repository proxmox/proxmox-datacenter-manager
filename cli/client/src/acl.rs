use anyhow::Error;

use proxmox_config_digest::PROXMOX_CONFIG_DIGEST_SCHEMA;
use proxmox_router::cli::{CliCommand, CliCommandMap, CommandLineInterface, OutputFormat};
use proxmox_schema::api;

use pdm_api_types::{AclUgidType, Authid, Role, ACL_PATH_SCHEMA, ACL_PROPAGATE_SCHEMA};
use pdm_client::{AclRecipient, ConfigDigest};

use crate::{client, env};

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
async fn list_acls(path: Option<String>, exact: bool) -> Result<(), Error> {
    let client = client()?;

    let (entries, _digest) = client.read_acl(path.as_deref(), exact).await?;

    let output_format = env().format_args.output_format;
    match output_format {
        OutputFormat::Json => println!("{}", serde_json::to_string(&entries)?),
        OutputFormat::JsonPretty => println!("{}", serde_json::to_string_pretty(&entries)?),
        OutputFormat::Text => {
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
