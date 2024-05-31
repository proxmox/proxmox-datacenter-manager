use anyhow::{bail, format_err, Error};
use futures::*;
use nix::sys::stat::{fchmodat, FchmodatFlags, Mode};
use nix::unistd::{fchownat, FchownatFlags};

use proxmox_lang::try_block;
use proxmox_rest_server::{daemon, ApiConfig, RestServer, UnixAcceptor};
use proxmox_router::RpcEnvironmentType;
use proxmox_sys::fs::CreateOptions;

use server::auth;

pub const PROXMOX_BACKUP_TCP_KEEPALIVE_TIME: u32 = 5 * 60;

fn main() -> Result<(), Error> {
    //pbs_tools::setup_libc_malloc_opts(); // TODO: move from PBS to proxmox-sys and uncomment

    server::env::sanitize_environment_vars();

    proxmox_product_config::init(pdm_config::api_user()?);

    let debug = std::env::var("PROXMOX_DEBUG").is_ok();

    if let Err(err) = syslog::init(
        syslog::Facility::LOG_DAEMON,
        if debug {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        },
        Some("proxmox-datacenter-manager-priv"),
    ) {
        bail!("unable to inititialize syslog - {err}");
    }

    create_directories()?;

    let mut args = std::env::args();
    args.next();
    for arg in args {
        match arg.as_ref() {
            "setup" => {
                let code = match server::auth::setup_keys() {
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

    proxmox_async::runtime::main(run())
}

fn create_directories() -> Result<(), Error> {
    let api_user = pdm_config::api_user()?;

    pdm_config::setup::create_configdir()?;

    pdm_config::setup::mkdir_perms(
        pdm_buildcfg::PDM_RUN_DIR,
        nix::unistd::ROOT,
        api_user.gid,
        0o1770,
    )?;

    pdm_config::setup::mkdir_perms(
        pdm_buildcfg::PDM_LOG_DIR,
        nix::unistd::ROOT,
        api_user.gid,
        0o755,
    )?;

    pdm_config::setup::mkdir_perms(
        concat!(pdm_buildcfg::PDM_LOG_DIR_M!(), "/api"),
        api_user.uid,
        api_user.gid,
        0o755,
    )?;

    Ok(())
}

async fn run() -> Result<(), Error> {
    auth::init(true);

    let api_user = pdm_config::api_user()?;
    let mut commando_sock =
        proxmox_rest_server::CommandSocket::new(proxmox_rest_server::our_ctrl_sock(), api_user.gid);

    let dir_opts = CreateOptions::new().owner(api_user.uid).group(api_user.gid);
    let file_opts = CreateOptions::new().owner(api_user.uid).group(api_user.gid);

    let config = ApiConfig::new(pdm_buildcfg::JS_DIR, RpcEnvironmentType::PRIVILEGED)
        .auth_handler_func(|h, m| Box::pin(auth::check_auth(h, m)))
        .formatted_router(&["api2"], &server::api::ROUTER)
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

    // FIXME: This should probably only happen at bind() time in proxmox-rest-server...
    match std::fs::remove_file(pdm_buildcfg::PDM_PRIVILEGED_API_SOCKET_FN) {
        Ok(()) => (),
        Err(err) if err.kind() == io::ErrorKind::NotFound => (),
        Err(err) => bail!("failed to remove old socket: {err}"),
    }
    let server = daemon::create_daemon(
        std::os::unix::net::SocketAddr::from_pathname(pdm_buildcfg::PDM_PRIVILEGED_API_SOCKET_FN)
            .expect("bad api socket path"),
        move |listener: tokio::net::UnixListener| {
            let sockpath = pdm_buildcfg::PDM_PRIVILEGED_API_SOCKET_FN;

            // NOTE: NoFollowSymlink is apparently not implemented in fchmodat()...
            fchmodat(
                Some(libc::AT_FDCWD),
                sockpath,
                Mode::from_bits_truncate(0o660),
                FchmodatFlags::FollowSymlink,
            )
            .map_err(|err| {
                format_err!("unable to set mode for api socket '{sockpath:?}' - {err}")
            })?;

            fchownat(
                None,
                sockpath,
                None,
                Some(api_user.gid),
                FchownatFlags::FollowSymlink,
            )
            .map_err(|err| {
                format_err!("unable to set ownership for api socket '{sockpath}' - {err}")
            })?;

            let incoming = UnixAcceptor::from(listener);

            Ok(async {
                daemon::systemd_notify(daemon::SystemdNotify::Ready)?;

                hyper::Server::builder(incoming)
                    .serve(rest_server)
                    .with_graceful_shutdown(proxmox_rest_server::shutdown_future())
                    .map_err(Error::from)
                    .await
            })
        },
        Some(pdm_buildcfg::PDM_PRIVILEGED_API_PID_FN),
    );

    proxmox_rest_server::write_pid(pdm_buildcfg::PDM_PRIVILEGED_API_PID_FN)?;

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
