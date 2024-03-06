use anyhow::{bail, format_err, Error};
use once_cell::sync::{Lazy, OnceCell};

use proxmox_client::{Client, TlsOptions};
use proxmox_login::Login;
use proxmox_router::cli::{run_cli_command_with_args, CliCommand, CliCommandMap, CliEnvironment};
use proxmox_schema::api;

use pdm_client::PdmClient;

pub mod env;
pub mod fido;
pub mod pve;
pub mod remotes;
pub mod tags;
pub mod user;

pub static XDG: Lazy<xdg::BaseDirectories> = Lazy::new(|| {
    xdg::BaseDirectories::new().expect("failed to initialize XDG base directory info")
});

static ENV: OnceCell<env::Env> = OnceCell::new();

pub fn env() -> &'static env::Env {
    // unwrap: initialized at startup
    ENV.get().unwrap()
}

pub fn client() -> Result<PdmClient<Client>, Error> {
    let address = format!(
        "https://{}:8443/",
        env()
            .server
            .as_ref()
            .ok_or_else(|| format_err!("no server address specified"))?
    )
    .parse()?;

    let options = TlsOptions::Callback(Box::new(|valid, store| {
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
    }));

    let userid = env().query_userid(&address)?;
    let client = Client::with_options(address.clone(), options, Default::default())?;

    if let Some(ticket) = env().load_ticket(&address, &userid)? {
        let auth: proxmox_client::Authentication = serde_json::from_slice(&ticket)?;
        client.set_authentication(auth);
    }

    Ok(PdmClient(client))
}

fn main() {
    //pbs_tools::setup_libc_malloc_opts(); // TODO: move from PBS to proxmox-sys and uncomment
    proxmox_router::cli::init_cli_logger("PDM_LOG", "info");

    match main_do() {
        Ok(()) => (),
        Err(err) => {
            eprintln!("{err:?}");
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
        .insert("pve", pve::cli())
        .insert("remote", remotes::cli())
        .insert("user", user::cli());

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
    let userid = env().query_userid(client.api_url())?;
    let password = env().query_password(client.api_url(), &userid)?;
    if let Some(tfa) = client
        .login(Login::new(client.api_url().to_string(), &userid, &password))
        .await?
    {
        let response = env().query_second_factor(client.api_url(), &userid, &tfa.challenge)?;
        let response = tfa.respond_raw(&response);
        client.login_tfa(tfa, response).await?;
    }

    if let Some(ticket) = client.serialize_ticket()? {
        env().store_ticket(client.api_url(), &userid, &ticket)?;
    }

    Ok(())
}
