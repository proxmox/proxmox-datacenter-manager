use anyhow::Error;
use serde_json::json;

//use proxmox_notify::context::pbs::PBS_CONTEXT;
use proxmox_router::{cli::*, ApiHandler, RpcEnvironment};
use proxmox_subscription::SubscriptionStatus;
use proxmox_sys::fs::CreateOptions;

use server::api;

async fn wait_for_local_worker(upid_str: &str) -> Result<(), Error> {
    let upid: pbs_api_types::UPID = upid_str.parse()?;
    let poll_delay = core::time::Duration::from_millis(100);

    loop {
        if !proxmox_rest_server::worker_is_active_local(&upid) {
            break;
        }
        tokio::time::sleep(poll_delay).await;
    }
    Ok(())
}

/// Daily update
async fn do_update(rpcenv: &mut dyn RpcEnvironment) -> Result<(), Error> {
    if let Err(err) = &api::nodes::subscription::check_subscription().await {
        log::error!("Error checking subscription - {err}");
    }
    match api::nodes::subscription::get_subscription().await {
        Ok(info) if info.info.status == SubscriptionStatus::Active => {}
        Ok(info) => {
            log::warn!(
                "Subscription not active: {status}",
                status = info.info.status
            )
        }
        Err(err) => {
            log::error!("Error reading subscription - {err}");
        }
    };

    println!("updating apt package database");
    let param = json!({
        "notify": true, // TODO: not yet supported anyway?!
    });
    let method = &api::nodes::apt::API_METHOD_APT_UPDATE_DATABASE;
    match method.handler {
        ApiHandler::Sync(handler) => match (handler)(param, method, rpcenv) {
            Err(err) => {
                log::error!("Error triggering apt database update - {err}");
            }
            Ok(upid) => wait_for_local_worker(upid.as_str().unwrap()).await?,
        },
        _ => unreachable!(),
    };

    println!("check if any ACME-managed certificate requires renewal");
    if let Err(err) = check_acme_certificates(rpcenv).await {
        log::error!("error checking certificates: {err}");
    }

    // TODO: cleanup tasks like in PVE?

    Ok(())
}

async fn check_acme_certificates(rpcenv: &mut dyn RpcEnvironment) -> Result<(), Error> {
    let (cert_config, _digest) = pdm_config::certificate_config::config()?;

    // do we even have any acme domains configures?
    if cert_config.acme_domains().next().is_none() {
        return Ok(());
    }

    if !api::nodes::certificates::cert_expires_soon()? {
        log::info!("Certificate does not expire within the next 30 days, not renewing.");
        return Ok(());
    }

    let info = &api::nodes::certificates::API_METHOD_RENEW_ACME_CERT;
    let result = match info.handler {
        ApiHandler::Sync(handler) => (handler)(json!({}), info, rpcenv)?,
        _ => unreachable!(),
    };
    wait_for_local_worker(result.as_str().unwrap()).await?;

    Ok(())
}

async fn run(rpcenv: &mut dyn RpcEnvironment) -> Result<(), Error> {
    let api_user = pdm_config::api_user()?;
    let file_opts = CreateOptions::new().owner(api_user.uid).group(api_user.gid);
    proxmox_rest_server::init_worker_tasks(pdm_buildcfg::PDM_LOG_DIR_M!().into(), file_opts)?;

    let mut command_sock = proxmox_daemon::command_socket::CommandSocket::new(api_user.gid);
    proxmox_rest_server::register_task_control_commands(&mut command_sock)?;
    command_sock.spawn(proxmox_rest_server::last_worker_future())?;

    //proxmox_notify::context::set_context(&PBS_CONTEXT);

    server::context::init()?;
    do_update(rpcenv).await
}

fn main() {
    if let Err(err) = syslog::init(
        syslog::Facility::LOG_DAEMON,
        log::LevelFilter::Warn,
        Some("proxmox-datacenter-manager-daily-update"),
    ) {
        eprintln!("unable to initialize syslog - {err}");
    }

    let mut rpcenv = CliEnvironment::new();
    rpcenv.set_auth_id(Some(String::from("root@pam")));

    if let Err(err) = proxmox_async::runtime::main(run(&mut rpcenv)) {
        log::error!("error during update: {err}");
        std::process::exit(1);
    }
}
