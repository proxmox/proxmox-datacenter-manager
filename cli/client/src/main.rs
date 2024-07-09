use std::io;
use std::sync::OnceLock;

use anyhow::{bail, format_err, Error};
use once_cell::sync::Lazy;

use proxmox_auth_api::types::Userid;
use proxmox_client::{Client, TlsOptions};
use proxmox_login::Login;
use proxmox_router::cli::{CliCommand, CliCommandMap, CliEnvironment, GlobalOptions};
use proxmox_schema::api;

use pdm_client::PdmClient;

#[macro_use]
pub mod env;

pub mod config;
pub mod pve;
pub mod remotes;
pub mod tags;
pub mod time;
pub mod user;

use config::PdmConnectArgs;

pub static XDG: Lazy<xdg::BaseDirectories> = Lazy::new(|| {
    xdg::BaseDirectories::new().expect("failed to initialize XDG base directory info")
});

static ENV: OnceLock<env::Env> = OnceLock::new();

pub fn env() -> &'static env::Env {
    // unwrap: initialized at startup
    ENV.get().unwrap()
}

pub fn client() -> Result<PdmClient<Client>, Error> {
    let address = env().url()?.parse()?;

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
    let mut env = env::Env::new()?;

    unsafe {
        libc::setlocale(libc::LC_ALL, [0].as_ptr());
    }

    let cmd_def = CliCommandMap::new()
        .global_option(GlobalOptions::of::<PdmConnectArgs>())
        .global_option(
            GlobalOptions::of::<config::FormattingArgs>()
                .completion_cb("color", env::complete_color),
        )
        .insert("login", CliCommand::new(&API_METHOD_LOGIN))
        .insert("pve", pve::cli())
        .insert("remote", remotes::cli())
        .insert("user", user::cli())
        .insert_help()
        .build();

    let mut rpcenv = CliEnvironment::new();

    let cli_parser = proxmox_router::cli::CommandLine::new(cmd_def)
        .with_async(|future| proxmox_async::runtime::main(future));
    let invocation = cli_parser.parse(&mut rpcenv, std::env::args())?;

    env.connect_args = rpcenv
        .take_global_option()
        .ok_or_else(|| format_err!("missing connect args"))?;
    env.connect_args.finalize()?;

    if ENV.set(env).is_err() {
        bail!("failed to initialize environment");
    }

    invocation.call(&mut rpcenv)
}

#[api]
/// Log into a server.
async fn login() -> Result<(), Error> {
    if env().connect_args.host.is_none() || env().connect_args.user.is_none() {
        bail!("no server chosen, please use the '--server=https://USER@HOST' parameter");
    }

    let client = client()?;
    let userid = env().query_userid(client.api_url())?;

    let login_how = 'login: {
        if let Some(server) = env().connect_args.host.as_deref() {
            if matches!(server, "localhost" | "127.0.0.1" | "::1") {
                if let Some(login_how) = try_create_local_ticket(&client, &userid)? {
                    break 'login login_how;
                }
            }
        }

        let password = env().query_password(client.api_url(), &userid)?;
        Login::new(client.api_url().to_string(), userid.as_str(), password)
    };

    if let Some(tfa) = client.login(login_how).await? {
        let response = env().query_second_factor(client.api_url(), &userid, &tfa.challenge)?;
        let response = tfa.respond_raw(&response);
        client.login_tfa(tfa, response).await?;
    }

    if let Some(ticket) = client.serialize_ticket()? {
        env().store_ticket(client.api_url(), &userid, &ticket)?;
    }

    Ok(())
}

fn try_create_local_ticket(
    client: &PdmClient<Client>,
    userid: &Userid,
) -> Result<Option<Login>, Error> {
    use proxmox_auth_api::api::ApiTicket;
    use proxmox_auth_api::ticket::Ticket;
    use proxmox_auth_api::{Keyring, PrivateKey};

    let authkey_path = pdm_buildcfg::configdir!("/auth/authkey.key");
    let keyring = match std::fs::read(authkey_path) {
        Err(err) => {
            if !matches!(
                err.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
            ) {
                log::error!("failed to read auth key from {authkey_path:?}: {err:?}");
            }
            return Ok(None);
        }
        Ok(pem) => Keyring::with_private_key(PrivateKey::from_pem(&pem)?),
    };

    let ticket = Ticket::new("PDM", &ApiTicket::Full(userid.clone()))?.sign(&keyring, None)?;

    Ok(Some(
        Login::renew(client.api_url().to_string(), ticket)
            .expect("failed to parse generated ticket"),
    ))
}
