use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::path::Path;
use std::pin::pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{bail, Context as _, Error};
use futures::*;
use http::request::Parts;
use http::Response;
use hyper::header;
use hyper::StatusCode;
use hyper_util::server::graceful::GracefulShutdown;
use openssl::ssl::SslAcceptor;
use serde_json::{json, Value};
use tracing::level_filters::LevelFilter;
use url::form_urlencoded;

use proxmox_lang::try_block;
use proxmox_rest_server::{ApiConfig, RestEnvironment, RestServer};
use proxmox_router::{RpcEnvironment, RpcEnvironmentType};
use proxmox_sys::fs::CreateOptions;

use pdm_buildcfg::configdir;

use pdm_api_types::Authid;
use proxmox_auth_api::api::assemble_csrf_prevention_token;

use server::auth;
use server::auth::csrf::csrf_secret;
use server::metric_collection;
use server::resource_cache;
use server::task_utils;

mod tasks;

pub const PROXMOX_BACKUP_TCP_KEEPALIVE_TIME: u32 = 5 * 60;

const PDM_LISTEN_ADDR: SocketAddr = SocketAddr::new(
    IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0)),
    pdm_buildcfg::PDM_PORT,
);

fn main() -> Result<(), Error> {
    //pbs_tools::setup_libc_malloc_opts(); // TODO: move from PBS to proxmox-sys and uncomment

    server::env::sanitize_environment_vars();

    let debug = std::env::var("PROXMOX_DEBUG").is_ok();

    proxmox_log::Logger::from_env("PROXMOX_DEBUG", LevelFilter::INFO)
        .journald_on_no_workertask()
        .tasklog_pbs()
        .init()?;

    if std::env::args().nth(1).is_some() {
        bail!("unexpected command line parameters");
    }

    let api_uid = pdm_config::api_user()?.uid;
    let api_gid = pdm_config::api_group()?.gid;
    let running_uid = nix::unistd::Uid::effective();
    let running_gid = nix::unistd::Gid::effective();

    if running_uid != api_uid || running_gid != api_gid {
        bail!("api not running as api user or group (got uid {running_uid} gid {running_gid})");
    }

    proxmox_product_config::init(pdm_config::api_user()?, pdm_config::priv_user()?);
    server::context::init()?;

    proxmox_async::runtime::main(run(debug))
}

async fn get_index_future(env: RestEnvironment, parts: Parts) -> Response<proxmox_http::Body> {
    let auth_id = env.get_auth_id();
    let api = env.api_config();

    // fixme: make all IO async

    let (userid, csrf_token) = match auth_id {
        Some(auth_id) => {
            let auth_id = auth_id.parse::<Authid>();
            match auth_id {
                Ok(auth_id) if !auth_id.is_token() => {
                    let userid = auth_id.user().clone();
                    let new_csrf_token = assemble_csrf_prevention_token(csrf_secret(), &userid);
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

async fn run(debug: bool) -> Result<(), Error> {
    auth::init(false);

    proxmox_acme_api::init(configdir!("/acme"), false)?;

    metric_collection::init()?;

    let api_user = pdm_config::api_user()?;
    let mut command_sock = proxmox_daemon::command_socket::CommandSocket::new(api_user.gid);

    let dir_opts = CreateOptions::new().owner(api_user.uid).group(api_user.gid);
    let file_opts = CreateOptions::new().owner(api_user.uid).group(api_user.gid);

    let indexpath = Path::new(pdm_buildcfg::JS_DIR).join("index.hbs");

    let config = ApiConfig::new(pdm_buildcfg::JS_DIR, RpcEnvironmentType::PUBLIC)
        .privileged_addr(
            std::os::unix::net::SocketAddr::from_pathname(
                pdm_buildcfg::PDM_PRIVILEGED_API_SOCKET_FN,
            )
            .expect("bad privileged socket path"),
        )
        .index_handler_func(|e, p| Box::pin(get_index_future(e, p)))
        .auth_handler_func(|h, m| Box::pin(auth::check_auth(h, m)))
        .register_template("index", &indexpath)?
        .register_template("console", "/usr/share/pve-xtermjs/index.html.hbs")?
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
        .formatted_router(&["api2"], &server::api::ROUTER)
        // FIXME: disabled for testing on pure debian
        //.register_template("console", "/usr/share/pve-xtermjs/index.html.hbs")?
        .enable_access_log(
            pdm_buildcfg::API_ACCESS_LOG_FN,
            Some(dir_opts),
            Some(file_opts),
            &mut command_sock,
        )?
        .enable_auth_log(
            pdm_buildcfg::API_AUTH_LOG_FN,
            Some(dir_opts),
            Some(file_opts),
            &mut command_sock,
        )?;

    let rest_server = RestServer::new(config);
    let redirector = proxmox_rest_server::Redirector::new();
    proxmox_rest_server::init_worker_tasks(pdm_buildcfg::PDM_LOG_DIR_M!().into(), file_opts)?;

    //openssl req -x509 -newkey rsa:4096 -keyout /etc/proxmox-backup/api.key -out /etc/proxmox-backup/api.pem -nodes

    // we build the initial acceptor here as we cannot start if this fails
    let acceptor = make_tls_acceptor()?;
    let acceptor = Arc::new(Mutex::new(acceptor));

    // to renew the acceptor we just add a command-socket handler
    command_sock.register_command("reload-certificate".to_string(), {
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

    let connections = proxmox_rest_server::connection::AcceptBuilder::new().debug(debug);
    let server = proxmox_daemon::server::create_daemon(
        PDM_LISTEN_ADDR,
        /*
        move |listener| {
            let (secure_connections, insecure_connections) =
                connections.accept_tls_optional(listener, acceptor);

            Ok(async {
                log::info!("service ready and listening at {PDM_LISTEN_ADDR}");
                proxmox_systemd::notify::SystemdNotify::Ready.notify()?;

                let secure_server = hyper::Server::builder(secure_connections)
                    .serve(rest_server)
                    .with_graceful_shutdown(proxmox_daemon::shutdown_future())
                    .map_err(Error::from);

                let insecure_server = hyper::Server::builder(insecure_connections)
                    .serve(redirector)
                    .with_graceful_shutdown(proxmox_daemon::shutdown_future())
                    .map_err(Error::from);

                let (secure_res, insecure_res) =
                    try_join!(tokio::spawn(secure_server), tokio::spawn(insecure_server))
                        .context("failed to complete REST server task")?;

                let mut err_msg = String::new();
                let mut is_err = false;
                for res in [secure_res, insecure_res] {
                    if let Err(err) = res {
                        use std::fmt::Write as _;

                        is_err = true;

                        if !err_msg.is_empty() {
                            err_msg.push('\n');
                        }
                        let _ = write!(err_msg, "{err}");
                    }
                }
                if is_err {
                    bail!(err_msg);
                }
                Ok(())
            })
        },
        */
        move |listener| {
            let (mut secure_connections, mut insecure_connections) =
                connections.accept_tls_optional(listener, acceptor);

            Ok(async {
                log::info!("service ready and listening at {PDM_LISTEN_ADDR}");
                proxmox_systemd::notify::SystemdNotify::Ready.notify()?;

                let secure_server = async move {
                    let graceful = GracefulShutdown::new();
                    loop {
                        tokio::select! {
                            Some(conn) = secure_connections.next() => {
                                match conn {
                                    Ok(conn) => {
                                        let api_service = rest_server.api_service(&conn)?;
                            let watcher = graceful.watcher();
                                        tokio::spawn(async move {
                                            api_service.serve(conn, Some(watcher)).await
                                        });
                                    },
                                    Err(err) => { log::warn!("Failed to accept insecure connection: {err:?}"); }
                                }
                            },
                            _shutdown = proxmox_daemon::shutdown_future() => {
                                break;
                            }
                        }
                    }
                    graceful.shutdown().await;
                    Ok::<(), Error>(())
                };

                let insecure_server = async move {
                    let graceful = GracefulShutdown::new();
                    loop {
                        tokio::select! {
                            Some(conn) = insecure_connections.next() => {
                                match conn {
                                    Ok(conn) => {
                                        let redirect_service = redirector.redirect_service();
                            let watcher = graceful.watcher();
                                        tokio::spawn(async move {
                                            redirect_service.serve(conn, Some(watcher)).await
                                        });
                                    },
                                    Err(err) => { log::warn!("Failed to accept insecure connection: {err:?}"); }
                                }
                            },
                            _shutdown = proxmox_daemon::shutdown_future() => {
                                break;
                            }
                        }
                    }
                    graceful.shutdown().await;
                    Ok::<(), Error>(())
                };

                let (secure_res, insecure_res) =
                    try_join!(tokio::spawn(secure_server), tokio::spawn(insecure_server))
                        .context("failed to complete REST server task")?;

                let results: [Result<(), Error>; 2] = [secure_res, insecure_res];

                if results.iter().any(Result::is_err) {
                    let cat_errors = results
                        .into_iter()
                        .filter_map(|res| res.err().map(|err| err.to_string()))
                        .collect::<Vec<_>>()
                        .join("\n");

                    bail!(cat_errors);
                }

                Ok::<(), Error>(())
            })
        },
        Some(pdm_buildcfg::PDM_API_PID_FN),
    );

    proxmox_rest_server::write_pid(pdm_buildcfg::PDM_API_PID_FN)?;

    let init_result: Result<(), Error> = try_block!({
        proxmox_rest_server::register_task_control_commands(&mut command_sock)?;
        command_sock.spawn(proxmox_rest_server::last_worker_future())?;
        proxmox_daemon::catch_shutdown_signal(proxmox_rest_server::last_worker_future())?;
        proxmox_daemon::catch_reload_signal(proxmox_rest_server::last_worker_future())?;
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
    metric_collection::start_task();
    tasks::remote_node_mapping::start_task();
    resource_cache::start_task();

    server.await?;
    log::info!("server shutting down, waiting for active workers to complete");
    proxmox_rest_server::last_worker_future().await;
    log::info!("done - exit server");

    Ok(())
}

fn make_tls_acceptor() -> Result<SslAcceptor, Error> {
    let key_path = configdir!("/auth/api.key");
    let cert_path = configdir!("/auth/api.pem");

    proxmox_rest_server::connection::TlsAcceptorBuilder::new()
        .certificate_paths_pem(key_path, cert_path)
        .build()
}

// TODO: move scheduling stuff to own module
fn start_task_scheduler() {
    tokio::spawn(async move {
        let task_scheduler = pin!(run_task_scheduler());
        let abort_future = pin!(proxmox_daemon::shutdown_future());
        futures::future::select(task_scheduler, abort_future).await;
    });
}

async fn run_task_scheduler() {
    loop {
        // sleep first to align to next minute boundary for first round
        let delay_target = task_utils::next_aligned_instant(60);
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
