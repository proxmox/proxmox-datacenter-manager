use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{bail, Error};
use futures::*;
use http::request::Parts;
use http::Response;
use hyper::header;
use hyper::{Body, StatusCode};
use url::form_urlencoded;

use openssl::ssl::SslAcceptor;
use serde_json::{json, Value};

use proxmox_lang::try_block;
use proxmox_router::{
    list_subdirs_api_method, Permission, Router, RpcEnvironment, RpcEnvironmentType, SubdirMap,
};
use proxmox_schema::api;
use proxmox_sortable_macro::sortable;
use proxmox_sys::fs::CreateOptions;

use proxmox_rest_server::{cookie_from_header, ApiConfig, RestEnvironment, RestServer};

use pdm_buildcfg::configdir;

use pdm_api_types::Authid;

use proxmox_rest_server::daemon;

use proxmox_datacenter_manager::auth;

pub const PROXMOX_BACKUP_TCP_KEEPALIVE_TIME: u32 = 5 * 60;

fn main() -> Result<(), Error> {
    //pbs_tools::setup_libc_malloc_opts(); // TODO: move from PBS to proxmox-sys and uncomment

    proxmox_datacenter_manager::env::sanitize_environment_vars();

    let mut args = std::env::args();
    args.next();
    for arg in args {
        match arg.as_ref() {
            "setup" => {
                let code = match auth::setup_keys() {
                    Ok(_) => 0,
                    Err(err) => {
                        eprintln!("got error on setup - {err}");
                        -1
                    }
                };
                std::process::exit(code);
            }
            _ => {
                eprintln!("did not understand argument {arg}");
            }
        }
    }

    let api_uid = pdm_config::api_user()?.uid;
    let api_gid = pdm_config::api_group()?.gid;
    let running_uid = nix::unistd::Uid::effective();
    let running_gid = nix::unistd::Gid::effective();

    if running_uid != api_uid || running_gid != api_gid {
        bail!("proxy not running as api user or group (got uid {running_uid} gid {running_gid})");
    }

    proxmox_async::runtime::main(run())
}

/// check for a cookie with the user-preferred language, fallback to the config one if not set or
/// not existing
fn get_language(headers: &http::HeaderMap) -> String {
    let exists = |l: &str| Path::new(&format!("/usr/share/pbs-i18n/pbs-lang-{l}.js")).exists();

    match cookie_from_header(headers, "PBSLangCookie") {
        Some(cookie_lang) if exists(&cookie_lang) => cookie_lang,
        _ => String::from(""),
    }
}

async fn get_index_future(env: RestEnvironment, parts: Parts) -> Response<Body> {
    let auth_id = env.get_auth_id();
    let api = env.api_config();

    // fixme: make all IO async

    let (userid, csrf_token) = match auth_id {
        Some(auth_id) => {
            let auth_id = auth_id.parse::<Authid>();
            match auth_id {
                Ok(auth_id) if !auth_id.is_token() => {
                    let userid = auth_id.user().clone();
                    let new_csrf_token = auth::csrf::assemble_csrf_prevention_token(&userid);
                    (Some(userid), Some(new_csrf_token))
                }
                _ => (None, None),
            }
        }
        None => (None, None),
    };

    let nodename = proxmox_sys::nodename();
    let user = userid.as_ref().map(|u| u.as_str()).unwrap_or("");

    let csrf_token = csrf_token.unwrap_or_else(|| String::from(""));

    let mut debug = false;
    let mut template_file = "index";

    if let Some(query_str) = parts.uri.query() {
        for (k, v) in form_urlencoded::parse(query_str.as_bytes()).into_owned() {
            if k == "debug" && v != "0" && v != "false" {
                debug = true;
            } else if k == "console" {
                template_file = "console";
            }
        }
    }

    let data = json!({
        "NodeName": nodename,
        "UserName": user,
        "CSRFPreventionToken": csrf_token,
        "language": get_language(&parts.headers),
        "debug": debug,
    });

    let (ct, index) = match api.render_template(template_file, &data) {
        Ok(index) => ("text/html", index),
        Err(err) => ("text/plain", format!("Error rendering template: {err}")),
    };

    let mut resp = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, ct)
        .body(index.into())
        .unwrap();

    if let Some(userid) = userid {
        resp.extensions_mut().insert(Authid::from((userid, None)));
    }

    resp
}

// FIXME: add actual API, and that in a separate module
#[api(
    access: {
        description: "Anyone can access this, just a cheap check if the API daemon is online.",
        permission: &Permission::World,
    }
)]
/// A simple ping method. returns "pong"
fn ping() -> Result<String, Error> {
    Ok("pong".to_string())
}

// FIXME: version should be only accessible to valid user (albeit no specific priv)
#[api(
    access: {
        description: "Anyone can access this.",
        permission: &Permission::World,
    }
)]
/// Return the program's version/release info
fn version() -> Result<Value, Error> {
    Ok(json!({
        "version": pdm_buildcfg::PROXMOX_PKG_VERSION,
        "release": pdm_buildcfg::PROXMOX_PKG_RELEASE,
        "repoid": pdm_buildcfg::PROXMOX_PKG_REPOID
    }))
}

#[sortable]
const SUBDIRS: SubdirMap = &sorted!([
    ("ping", &Router::new().get(&API_METHOD_PING)),
    ("version", &Router::new().get(&API_METHOD_VERSION)),
]);

const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

async fn run() -> Result<(), Error> {
    let debug = std::env::var("PROXMOX_DEBUG").is_ok();

    if let Err(err) = syslog::init(
        syslog::Facility::LOG_DAEMON,
        if debug {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        },
        Some("proxmox-datacenter-manager-proxy"),
    ) {
        bail!("unable to inititialize syslog - {err}");
    }

    auth::init();

    let api_user = pdm_config::api_user()?;
    let mut commando_sock =
        proxmox_rest_server::CommandSocket::new(proxmox_rest_server::our_ctrl_sock(), api_user.gid);

    let dir_opts = CreateOptions::new().owner(api_user.uid).group(api_user.gid);
    let file_opts = CreateOptions::new().owner(api_user.uid).group(api_user.gid);

    let config = ApiConfig::new(pdm_buildcfg::JS_DIR, RpcEnvironmentType::PUBLIC)
        .index_handler_func(|e, p| Box::pin(get_index_future(e, p)))
        .auth_handler_func(|h, m| Box::pin(auth::check_auth(h, m)))
        .aliases([
            ("extjs", "/usr/share/javascript/extjs"),
            ("qrcodejs", "/usr/share/javascript/qrcodejs"),
            ("fontawesome", "/usr/share/fonts-font-awesome"),
            ("xtermjs", "/usr/share/pve-xtermjs"),
            ("locale", "/usr/share/pdm-i18n"),
            (
                "proxmox-extjs-widget-toolkit",
                "/usr/share/javascript/proxmox-widget-toolkit",
            ),
            ("docs", "/usr/share/doc/proxmox-datacenter-manager/html"),
        ])
        .formatted_router(&["api2"], &ROUTER)
        .register_template("console", "/usr/share/pve-xtermjs/index.html.hbs")?
        .enable_access_log(
            pdm_buildcfg::API_ACCESS_LOG_FN,
            Some(dir_opts.clone()),
            Some(file_opts.clone()),
            &mut commando_sock,
        )?
        .enable_auth_log(
            pdm_buildcfg::API_AUTH_LOG_FN,
            Some(dir_opts.clone()),
            Some(file_opts.clone()),
            &mut commando_sock,
        )?;

    let rest_server = RestServer::new(config);
    proxmox_rest_server::init_worker_tasks(
        pdm_buildcfg::PDM_LOG_DIR_M!().into(),
        file_opts.clone(),
    )?;

    //openssl req -x509 -newkey rsa:4096 -keyout /etc/proxmox-backup/proxy.key -out /etc/proxmox-backup/proxy.pem -nodes

    // we build the initial acceptor here as we cannot start if this fails
    let acceptor = make_tls_acceptor()?;
    let acceptor = Arc::new(Mutex::new(acceptor));

    // to renew the acceptor we just add a command-socket handler
    commando_sock.register_command("reload-certificate".to_string(), {
        let acceptor = Arc::clone(&acceptor);
        move |_value| -> Result<_, Error> {
            log::info!("reloading certificate");
            match make_tls_acceptor() {
                Err(err) => log::error!("error reloading certificate: {err}"),
                Ok(new_acceptor) => {
                    let mut guard = acceptor.lock().unwrap();
                    *guard = new_acceptor;
                }
            }
            Ok(Value::Null)
        }
    })?;

    let connections =
        proxmox_rest_server::connection::AcceptBuilder::with_acceptor(acceptor).debug(debug);
    let server = daemon::create_daemon(
        ([0, 0, 0, 0, 0, 0, 0, 0], 8443).into(),
        move |listener| {
            let connections = connections.accept(listener);

            Ok(async {
                daemon::systemd_notify(daemon::SystemdNotify::Ready)?;

                hyper::Server::builder(connections)
                    .serve(rest_server)
                    .with_graceful_shutdown(proxmox_rest_server::shutdown_future())
                    .map_err(Error::from)
                    .await
            })
        },
        Some(pdm_buildcfg::PDM_PROXY_PID_FN),
    );

    proxmox_rest_server::write_pid(pdm_buildcfg::PDM_PROXY_PID_FN)?;

    let init_result: Result<(), Error> = try_block!({
        proxmox_rest_server::register_task_control_commands(&mut commando_sock)?;
        commando_sock.spawn()?;
        proxmox_rest_server::catch_shutdown_signal()?;
        proxmox_rest_server::catch_reload_signal()?;
        Ok(())
    });

    if let Err(err) = init_result {
        bail!("unable to start daemon - {err}");
    }

    // stop gap for https://github.com/tokio-rs/tokio/issues/4730 where the thread holding the
    // IO-driver may block progress completely if it starts polling its own tasks (blocks).
    // So, trigger a notify to parked threads, as we're immediately ready the woken up thread will
    // acquire the IO driver, if blocked, before going to sleep, which allows progress again
    // TODO: remove once tokio solves this at their level (see proposals in linked comments)
    let rt_handle = tokio::runtime::Handle::current();
    std::thread::spawn(move || loop {
        rt_handle.spawn(std::future::ready(()));
        std::thread::sleep(Duration::from_secs(3));
    });

    start_task_scheduler();

    server.await?;
    log::info!("server shutting down, waiting for active workers to complete");
    proxmox_rest_server::last_worker_future().await?;
    log::info!("done - exit server");

    Ok(())
}

fn make_tls_acceptor() -> Result<SslAcceptor, Error> {
    let key_path = configdir!("/auth/proxy.key");
    let cert_path = configdir!("/auth/proxy.pem");

    proxmox_rest_server::connection::TlsAcceptorBuilder::new()
        .certificate_paths_pem(key_path, cert_path)
        .build()
}

// TODO: move scheduling stuff to own module
fn start_task_scheduler() {
    let abort_future = proxmox_rest_server::shutdown_future();
    let future = Box::pin(run_task_scheduler());
    let task = futures::future::select(future, abort_future);
    tokio::spawn(task.map(|_| ()));
}

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn next_minute() -> Instant {
    let now = SystemTime::now();
    let epoch_now = match now.duration_since(UNIX_EPOCH) {
        Ok(d) => d,
        Err(err) => {
            eprintln!("task scheduler: compute next minute alignment failed - {err}");
            return Instant::now() + Duration::from_secs(60);
        }
    };
    let epoch_next = Duration::from_secs((epoch_now.as_secs() / 60 + 1) * 60);
    Instant::now() + epoch_next - epoch_now
}

async fn run_task_scheduler() {
    loop {
        // sleep first to align to next minute boundary for first round
        let delay_target = next_minute();
        tokio::time::sleep_until(tokio::time::Instant::from_std(delay_target)).await;

        match schedule_tasks().catch_unwind().await {
            Err(panic) => match panic.downcast::<&str>() {
                Ok(msg) => eprintln!("task scheduler panic: {msg}"),
                Err(_) => eprintln!("task scheduler panic - unknown type"),
            },
            Ok(Err(err)) => eprintln!("task scheduler failed - {err:?}"),
            Ok(Ok(_)) => {}
        }
    }
}

async fn schedule_tasks() -> Result<(), Error> {
    // TODO: move out to own module, refactor PBS stuff for reuse & then add:
    // - task log rotation
    // - stats (rrd) collection
    // - ...?

    Ok(())
}
