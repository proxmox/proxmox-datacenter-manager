use anyhow::{format_err, Error};

use proxmox_lang::try_block;
use proxmox_rest_server::WorkerTask;
use proxmox_sys::logrotate::LogRotate;
use proxmox_time::CalendarEvent;

use pdm_api_types::Authid;
use server::jobstate::{self, Job, JobState};

/// Rotate task logs, auth logs and access logs.
///
/// This task runs every day at midnight, except when it has never run before, then it runs
/// immediately.
pub async fn schedule_task_log_rotate() {
    let worker_type = "logrotate";
    let job_id = "access-log_and_task-archive";

    // schedule daily at 00:00 like normal logrotate
    let schedule = "00:00";

    if !check_schedule(worker_type, schedule, job_id) {
        // if we never ran the rotation, schedule instantly
        match JobState::load(worker_type, job_id) {
            Ok(JobState::Created { .. }) => {}
            _ => return,
        }
    }

    let mut job = match Job::new(worker_type, job_id) {
        Ok(job) => job,
        Err(_) => return, // could not get lock
    };

    if let Err(err) = WorkerTask::new_thread(
        worker_type,
        None,
        Authid::root_auth_id().to_string(),
        false,
        move |worker| {
            job.start(&worker.upid().to_string())?;
            proxmox_log::info!("starting task log rotation");

            let result = try_block!({
                let max_size = 512 * 1024 - 1; // an entry has ~ 100b, so > 5000 entries/file
                let max_files = 20; // times twenty files gives > 100000 task entries

                // TODO: Make this configurable
                let max_days = None;

                let options = proxmox_product_config::default_create_options();

                let has_rotated = proxmox_rest_server::rotate_task_log_archive(
                    max_size,
                    true,
                    Some(max_files),
                    max_days,
                    Some(options),
                )?;

                if has_rotated {
                    log::info!("task log archive was rotated");
                } else {
                    log::info!("task log archive was not rotated");
                }

                let max_size = 32 * 1024 * 1024 - 1;
                let max_files = 14;

                let mut logrotate = LogRotate::new(
                    pdm_buildcfg::API_ACCESS_LOG_FN,
                    true,
                    Some(max_files),
                    Some(options),
                )?;

                if logrotate.rotate(max_size)? {
                    println!("rotated access log, telling daemons to re-open log file");
                    proxmox_async::runtime::block_on(command_reopen_access_logfiles())?;
                    log::info!("API access log was rotated");
                } else {
                    log::info!("API access log was not rotated");
                }

                let mut logrotate = LogRotate::new(
                    pdm_buildcfg::API_AUTH_LOG_FN,
                    true,
                    Some(max_files),
                    Some(options),
                )?;

                if logrotate.rotate(max_size)? {
                    println!("rotated auth log, telling daemons to re-open log file");
                    proxmox_async::runtime::block_on(command_reopen_auth_logfiles())?;
                    log::info!("API authentication log was rotated");
                } else {
                    log::info!("API authentication log was not rotated");
                }

                if has_rotated {
                    log::info!("cleaning up old task logs");
                    if let Err(err) = proxmox_rest_server::cleanup_old_tasks(true) {
                        log::warn!("could not completely cleanup old tasks: {err}");
                    }
                }

                Ok(())
            });

            let status = worker.create_state(&result);

            if let Err(err) = job.finish(status) {
                eprintln!("could not finish job state for {worker_type}: {err}");
            }

            result
        },
    ) {
        eprintln!("unable to start task log rotation: {err}");
    }
}

async fn command_reopen_access_logfiles() -> Result<(), Error> {
    // only care about the most recent daemon instance for each, proxy & api, as other older ones
    // should not respond to new requests anyway, but only finish their current one and then exit.
    let sock = proxmox_daemon::command_socket::this_path();
    let f1 =
        proxmox_daemon::command_socket::send_raw(sock, "{\"command\":\"api-access-log-reopen\"}\n");

    let pid = proxmox_rest_server::read_pid(pdm_buildcfg::PDM_API_PID_FN)?;
    let sock = proxmox_daemon::command_socket::path_from_pid(pid);
    let f2 =
        proxmox_daemon::command_socket::send_raw(sock, "{\"command\":\"api-access-log-reopen\"}\n");

    match futures::join!(f1, f2) {
        (Err(e1), Err(e2)) => Err(format_err!(
            "reopen commands failed, proxy: {e1}; api: {e2}"
        )),
        (Err(e1), Ok(_)) => Err(format_err!("reopen commands failed, proxy: {e1}")),
        (Ok(_), Err(e2)) => Err(format_err!("reopen commands failed, api: {e2}")),
        _ => Ok(()),
    }
}

async fn command_reopen_auth_logfiles() -> Result<(), Error> {
    // only care about the most recent daemon instance for each, proxy & api, as other older ones
    // should not respond to new requests anyway, but only finish their current one and then exit.
    let sock = proxmox_daemon::command_socket::this_path();
    let f1 =
        proxmox_daemon::command_socket::send_raw(sock, "{\"command\":\"api-auth-log-reopen\"}\n");

    let pid = proxmox_rest_server::read_pid(pdm_buildcfg::PDM_API_PID_FN)?;
    let sock = proxmox_daemon::command_socket::path_from_pid(pid);
    let f2 =
        proxmox_daemon::command_socket::send_raw(sock, "{\"command\":\"api-auth-log-reopen\"}\n");

    match futures::join!(f1, f2) {
        (Err(e1), Err(e2)) => Err(format_err!(
            "reopen commands failed, proxy: {e1}; api: {e2}"
        )),
        (Err(e1), Ok(_)) => Err(format_err!("reopen commands failed, proxy: {e1}")),
        (Ok(_), Err(e2)) => Err(format_err!("reopen commands failed, api: {e2}")),
        _ => Ok(()),
    }
}

fn check_schedule(worker_type: &str, event_str: &str, id: &str) -> bool {
    let event: CalendarEvent = match event_str.parse() {
        Ok(event) => event,
        Err(err) => {
            eprintln!("unable to parse schedule '{event_str}' - {err}");
            return false;
        }
    };

    let last = match jobstate::last_run_time(worker_type, id) {
        Ok(time) => time,
        Err(err) => {
            eprintln!("could not get last run time of {worker_type} {id}: {err}");
            return false;
        }
    };

    let next = match event.compute_next_event(last) {
        Ok(Some(next)) => next,
        Ok(None) => return false,
        Err(err) => {
            eprintln!("compute_next_event for '{event_str}' failed - {err}");
            return false;
        }
    };

    let now = proxmox_time::epoch_i64();
    next <= now
}
