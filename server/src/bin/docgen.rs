use anyhow::{bail, Error};

use proxmox_schema::format::dump_enum_properties;
use proxmox_schema::ApiType;
use proxmox_section_config::{dump_section_config, typed::ApiSectionDataEntry};

use pdm_api_types::PRIVILEGES;

use server::api;

fn get_args() -> (String, Vec<String>) {
    let mut args = std::env::args();
    let prefix = args.next().unwrap();
    let prefix = prefix.rsplit('/').next().unwrap().to_string(); // without path
    let args: Vec<String> = args.collect();

    (prefix, args)
}

fn main() -> Result<(), Error> {
    let (_prefix, args) = get_args();

    if args.is_empty() {
        bail!("missing arguments");
    }

    for arg in args.iter() {
        let text = match arg.as_ref() {
            "apidata.js" => generate_api_tree(),
            "domains.cfg" => dump_section_config(&pdm_config::domains::CONFIG),
            //TODO: needs pub changes in proxmox-access-control
            //"user.cfg" => dump_section_config(&proxmox_access_control::user::CONFIG)
            "remotes.cfg" => dump_section_config(pdm_api_types::remotes::Remote::section_config()),
            "views.cfg" => dump_section_config(pdm_api_types::views::ViewConfigEntry::section_config()),
            "config::acl::Role" => dump_enum_properties(&pdm_api_types::Role::API_SCHEMA)?,
            _ => bail!("docgen: got unknown type"),
        };
        println!("{}", text);
    }

    Ok(())
}

fn generate_api_tree() -> String {
    let mut tree = Vec::new();

    let mut data = proxmox_docgen::generate_api_tree(&api::ROUTER, ".", PRIVILEGES);
    data["path"] = "/".into();
    // hack: add invisible space to sort as first entry
    data["text"] = "&#x200b;Management API (HTTP)".into();
    tree.push(data);

    format!(
        "var apiSchema = {};",
        serde_json::to_string_pretty(&tree).unwrap()
    )
}
