use anyhow::Error;

use proxmox_config_digest::ConfigDigest;
use proxmox_router::{Permission, Router, RpcEnvironment};
use proxmox_schema::api;

use pdm_api_types::{PRIV_SYS_AUDIT, PRIV_SYS_MODIFY};
use pdm_buildcfg::configdir;

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_GET_NOTES)
    .put(&API_METHOD_UPDATE_NOTES);

pub const NOTES_FILENAME: &str = configdir!("/notes.md");

fn read_notes() -> Result<(String, ConfigDigest), Error> {
    let notes = proxmox_sys::fs::file_read_optional_string(NOTES_FILENAME)?.unwrap_or_default();
    let digest = openssl::sha::sha256(notes.as_bytes());

    Ok((notes, digest.into()))
}

#[api(
    access: {
        permission: &Permission::Privilege(&["system"], PRIV_SYS_AUDIT, false),
    },
    returns: {
        description: "Notes, utf8 encoded markdown file.",
        type: String,
    },
    protected: true,
)]
/// Get notes.
pub fn get_notes(rpcenv: &mut dyn RpcEnvironment) -> Result<String, Error> {
    let (notes, digest) = read_notes()?;
    rpcenv["digest"] = digest.to_hex().into();
    Ok(notes)
}

#[api(
    input: {
        properties: {
            notes: {
                description: "New notes text.",
                type: String,
            },
            digest: {
                type: ConfigDigest,
                optional: true,
            },
        },
    },
    access: {
        // fixme: maybe we want something else here ...
        permission: &Permission::Privilege(&["system", "notes"], PRIV_SYS_MODIFY, false),
    }
)]
/// Update Notes
pub fn update_notes(notes: String, digest: Option<ConfigDigest>) -> Result<(), Error> {
    if digest.is_some() {
        let (_old, expected_digest) = read_notes()?;
        expected_digest.detect_modification(digest.as_ref())?;
    }
    proxmox_product_config::replace_config(NOTES_FILENAME, notes.as_bytes())
}
