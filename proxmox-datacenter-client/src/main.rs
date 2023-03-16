use anyhow::{bail, format_err, Error};
use once_cell::sync::{Lazy, OnceCell};

use proxmox_router::cli::{run_cli_command_with_args, CliCommand, CliCommandMap, CliEnvironment};
use proxmox_schema::api;

#[macro_use]
mod macros; // must go first

pub mod env;
pub mod fido;
pub mod remotes;

pub type Client = pdm_client::Client<&'static env::Env>;

pub static XDG: Lazy<xdg::BaseDirectories> = Lazy::new(|| {
    xdg::BaseDirectories::new().expect("failed to initialize XDG base directory info")
});

static ENV: OnceCell<env::Env> = OnceCell::new();

pub fn env() -> &'static env::Env {
    // unwrap: initialized at startup
    ENV.get().unwrap()
}

pub fn client() -> Result<Client, Error> {
    let address = format!(
        "https://{}:8443/",
        env()
            .server
            .as_ref()
            .ok_or_else(|| format_err!("no server address specified"))?
    );

    let options = pdm_client::Options::default().tls_callback(|valid, store| {
        if valid {
            return true;
        }

        match env().verify_cert(store) {
            Ok(b) => b,
            Err(err) => {
                eprintln!("failed to validate TLS certificate: {err}");
                false
            }
        }
    });

    Client::new(env(), &address, options)
}

fn main() {
    //pbs_tools::setup_libc_malloc_opts(); // TODO: move from PBS to proxmox-sys and uncomment
    proxmox_router::cli::init_cli_logger("PDM_LOG", "info");

    match main_do() {
        Ok(()) => (),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}

fn main_do() -> Result<(), Error> {
    let (env, args) = env::Env::from_args(std::env::args())?;
    if ENV.set(env).is_err() {
        bail!("failed to initialize environment");
    }

    let cmd_def = CliCommandMap::new()
        .insert("login", CliCommand::new(&API_METHOD_LOGIN))
        .insert("remote", remotes::cli());

    let rpcenv = CliEnvironment::new();
    run_cli_command_with_args(
        cmd_def,
        rpcenv,
        Some(|future| proxmox_async::runtime::main(future)),
        args,
    );

    Ok(())
}

#[api]
/// Log into a server.
async fn login() -> Result<(), Error> {
    if env().server.is_none() || env().userid.is_none() {
        bail!("no server chosen, please use the '--server=https://USER@HOST' parameter");
    }

    let client = client()?;
    client.login().await?;

    Ok(())
}

/*
fn parse_fingerprint(s: &str) -> Result<[u8; 32], Error> {
    use hex::FromHex;

    let hex: Vec<u8> = s
        .as_bytes()
        .iter()
        .copied()
        .filter(|&b| b != b':')
        .collect();

    <[u8; 32]>::from_hex(&hex).map_err(|_| format_err!("failed to parse fingerprint"))
}
*/
