//! Server/Node Configuration and Administration

use std::net::TcpListener;
use std::os::unix::io::AsRawFd;

use anyhow::{bail, format_err, Error};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, BufReader};

use proxmox_auth_api::ticket::{Empty, Ticket};
use proxmox_auth_api::types::Authid;
use proxmox_auth_api::Keyring;

use proxmox_rest_server::WorkerTask;
use proxmox_router::{Permission, Router, RpcEnvironment};
use proxmox_schema::*;
use proxmox_sys::fd::fd_change_cloexec;

use pdm_api_types::{NODE_SCHEMA, PRIV_SYS_CONSOLE};
use tracing::{info, warn};

pub const SHELL_CMD_SCHEMA: Schema = StringSchema::new("The command to run.")
    .format(&ApiStringFormat::Enum(&[
        EnumEntry::new("login", "Login"),
        EnumEntry::new("upgrade", "Upgrade"),
    ]))
    .schema();

#[api(
    protected: true,
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
            cmd: {
                schema: SHELL_CMD_SCHEMA,
                optional: true,
            },
        },
    },
    returns: {
        type: Object,
        description: "Object with the user, ticket, port and upid",
        properties: {
            user: {
                description: "",
                type: String,
            },
            ticket: {
                description: "",
                type: String,
            },
            port: {
                description: "",
                type: String,
            },
            upid: {
                description: "",
                type: String,
            },
        }
    },
    access: {
        description: "Restricted to users on realm 'pam'",
        permission: &Permission::Privilege(&["system"], PRIV_SYS_CONSOLE, false),
    }
)]
/// Call termproxy and return shell ticket
async fn termproxy(cmd: Option<String>, rpcenv: &mut dyn RpcEnvironment) -> Result<Value, Error> {
    // intentionally user only for now
    let auth_id: Authid = rpcenv
        .get_auth_id()
        .ok_or_else(|| format_err!("no authid available"))?
        .parse()?;

    if auth_id.is_token() {
        bail!("API tokens cannot access this API endpoint");
    }

    let userid = auth_id.user();

    if userid.realm() != "pam" {
        bail!("only pam users can use the console");
    }

    let path = "/system";

    // use port 0 and let the kernel decide which port is free
    let listener = TcpListener::bind("localhost:0")?;
    let port = listener.local_addr()?.port();

    let private_auth_keyring =
        Keyring::with_private_key(crate::auth::key::private_auth_key().clone());

    let ticket = Ticket::new(crate::auth::TERM_PREFIX, &Empty)?.sign(
        &private_auth_keyring,
        Some(&format!("{}{}{}", userid, path, port)),
    )?;

    let mut command = Vec::new();
    match cmd.as_deref() {
        Some("login") | None => {
            command.push("login");
            if userid == "root@pam" {
                command.push("-f");
                command.push("root");
            }
        }
        Some("upgrade") => {
            if userid != "root@pam" {
                bail!("only root@pam can upgrade");
            }
            // TODO: add nicer/safer wrapper like in PVE instead
            command.push("sh");
            command.push("-c");
            command.push("apt full-upgrade; bash -l");
        }
        _ => bail!("invalid command"),
    };

    let username = userid.name().to_owned();
    let upid = WorkerTask::spawn(
        "termproxy",
        None,
        auth_id.to_string(),
        false,
        move |worker| async move {
            // move inside the worker so that it survives and does not close the port
            // remove CLOEXEC from listenere so that we can reuse it in termproxy
            fd_change_cloexec(listener.as_raw_fd(), false)?;

            let mut arguments: Vec<&str> = Vec::new();
            let fd_string = listener.as_raw_fd().to_string();
            arguments.push(&fd_string);
            arguments.extend_from_slice(&[
                "--path",
                path,
                "--perm",
                "Sys.Console",
                "--authport",
                "82",
                "--port-as-fd",
                "--",
            ]);
            arguments.extend_from_slice(&command);

            let mut cmd = tokio::process::Command::new("/usr/bin/termproxy");

            cmd.args(&arguments)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());

            let mut child = cmd.spawn().expect("error executing termproxy");

            let stdout = child.stdout.take().expect("no child stdout handle");
            let stderr = child.stderr.take().expect("no child stderr handle");

            let stdout_fut = async move {
                let mut reader = BufReader::new(stdout).lines();
                while let Some(line) = reader.next_line().await? {
                    info!(line);
                }
                Ok::<(), Error>(())
            };

            let stderr_fut = async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Some(line) = reader.next_line().await? {
                    warn!(line);
                }
                Ok::<(), Error>(())
            };

            let mut needs_kill = false;
            let res = tokio::select! {
                res = child.wait() => {
                    let exit_code = res?;
                    if !exit_code.success() {
                        match exit_code.code() {
                            Some(code) => bail!("termproxy exited with {}", code),
                            None => bail!("termproxy exited by signal"),
                        }
                    }
                    Ok(())
                },
                res = stdout_fut => res,
                res = stderr_fut => res,
                res = worker.abort_future() => {
                    needs_kill = true;
                    res.map_err(Error::from)
                }
            };

            if needs_kill {
                if res.is_ok() {
                    child.kill().await?;
                    return Ok(());
                }

                if let Err(err) = child.kill().await {
                    warn!("error killing termproxy: {err}");
                } else if let Err(err) = child.wait().await {
                    warn!("error awaiting termproxy: {err}");
                }
            }

            res
        },
    )?;

    // FIXME: We're returning the user NAME only?
    Ok(json!({
        "user": username,
        "ticket": ticket,
        "port": port,
        "upid": upid,
    }))
}

pub const ROUTER: Router = Router::new().post(&API_METHOD_TERMPROXY);
