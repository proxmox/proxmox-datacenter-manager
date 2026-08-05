use anyhow::{Error, bail};

use proxmox_router::cli::*;
use proxmox_schema::api;
use server::api;
use server::auth::certs::update_self_signed_cert;
use server::auth::csrf::generate_csrf_key;
use server::auth::key::generate_auth_key;

#[api]
/// Display node certificate information.
fn cert_info() -> Result<(), Error> {
    let Some(cert) = api::nodes::certificates::get_info()?.pop() else {
        return Ok(());
    };

    println!("Subject: {}", cert.subject);

    for name in cert.san {
        println!("    {name}");
    }

    let not_before = cert
        .notbefore
        .and_then(|e| proxmox_time::strftime_utc("%b %e %T %Y %Z", e).ok());

    let not_after = cert
        .notafter
        .and_then(|e| proxmox_time::strftime_utc("%b %e %T %Y %Z", e).ok());

    println!("Issuer: {}", cert.issuer);
    println!("Validity:");
    println!("    Not Before: {}", not_before.unwrap_or_default());
    println!("    Not After : {}", not_after.unwrap_or_default());

    println!(
        "Fingerprint (sha256): {}",
        cert.fingerprint.unwrap_or_default()
    );

    println!("Public key type: {}", cert.public_key_type);
    println!(
        "Public key bits: {}",
        cert.public_key_bits.unwrap_or_default()
    );

    Ok(())
}

#[api(
    input: {
        properties: {
            force: {
                description: "Force generation of new SSL certificate.",
                type:  Boolean,
                optional:true,
            },
        }
    },
)]
/// Update node certificates and generate all needed files/directories. If no authentication key or
/// CSRF secret key exists, this will also generate new ones. These two keys will go into effect the
/// next time the `proxmox-datacenter-api.service` is started.
fn update_certs(force: Option<bool>) -> Result<(), Error> {
    pdm_config::setup::create_configdir()?;

    if let Err(err) = generate_auth_key() {
        bail!("unable to generate auth key - {err}");
    }

    if let Err(err) = generate_csrf_key() {
        bail!("unable to generate csrf key - {err}");
    }

    update_self_signed_cert(force.unwrap_or(false))?;

    Ok(())
}

pub fn cert_mgmt_cli() -> CommandLineInterface {
    let cmd_def = CliCommandMap::new()
        .insert("info", CliCommand::new(&API_METHOD_CERT_INFO))
        .insert("update", CliCommand::new(&API_METHOD_UPDATE_CERTS));

    cmd_def.into()
}
