use anyhow::{bail, Error};
use serde_json::Value;

use proxmox_apt_api_types::{
    APTChangeRepositoryOptions, APTGetChangelogOptions, APTRepositoriesResult, APTRepositoryHandle,
    APTUpdateInfo, APTUpdateOptions,
};
use proxmox_http::ProxyConfig;
use proxmox_rest_server::WorkerTask;
use proxmox_router::{
    list_subdirs_api_method, Permission, Router, RpcEnvironment, RpcEnvironmentType, SubdirMap,
};
use proxmox_schema::api;
use proxmox_schema::api_types::NODE_SCHEMA;
use proxmox_sys::fs::{replace_file, CreateOptions};

use proxmox_config_digest::ConfigDigest;

// use proxmox_http::ProxyConfig;

use pdm_api_types::{PRIV_SYS_AUDIT, PRIV_SYS_MODIFY};

#[api(
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
        },
    },
    returns: {
        description: "A list of packages with available updates.",
        type: Array,
        items: {
            type: APTUpdateInfo
        },
    },
    protected: true,
    access: {
        permission: &Permission::Privilege(&[], PRIV_SYS_AUDIT, false),
    },
)]
/// List available APT updates
fn apt_update_available(_param: Value) -> Result<Vec<APTUpdateInfo>, Error> {
    proxmox_apt::list_available_apt_update(pdm_buildcfg::APT_PKG_STATE_FN)
}

pub fn update_apt_proxy_config(proxy_config: Option<&ProxyConfig>) -> Result<(), Error> {
    const PROXY_CFG_FN: &str = "/etc/apt/apt.conf.d/76pveproxy"; // use same file as PVE

    if let Some(proxy_config) = proxy_config {
        let proxy = proxy_config.to_proxy_string()?;
        let data = format!("Acquire::http::Proxy \"{}\";\n", proxy);
        replace_file(PROXY_CFG_FN, data.as_bytes(), CreateOptions::new(), false)
    } else {
        match std::fs::remove_file(PROXY_CFG_FN) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => bail!("failed to remove proxy config '{}' - {}", PROXY_CFG_FN, err),
        }
    }
}

fn read_and_update_proxy_config() -> Result<Option<ProxyConfig>, Error> {
    let proxy_config = if let Ok((node_config, _digest)) = pdm_config::node::config() {
        pdm_config::node::get_http_proxy_config(&node_config)
    } else {
        None
    };
    update_apt_proxy_config(proxy_config.as_ref())?;

    Ok(proxy_config)
}

#[api(
    protected: true,
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
            options: {
                type: APTUpdateOptions,
                flatten: true,
            }
        },
    },
    returns: {
        schema:  proxmox_schema::upid::UPID_SCHEMA,
    },
    access: {
        permission: &Permission::Privilege(&[], PRIV_SYS_MODIFY, false),
    },
)]
/// Update the APT database
pub fn apt_update_database(
    options: APTUpdateOptions,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<String, Error> {
    let auth_id = rpcenv.get_auth_id().unwrap();
    let to_stdout = rpcenv.env_type() == RpcEnvironmentType::CLI;

    let upid_str = WorkerTask::new_thread("aptupdate", None, auth_id, to_stdout, move |_worker| {
        // fixme: read_and_update_proxy_config()?;

        proxmox_apt::update_database(
            pdm_buildcfg::APT_PKG_STATE_FN,
            &options,
            |_updates: &[&APTUpdateInfo]| {
                // fixme: add update notification
                // crate::server::send_updates_available(updates);
                Ok(())
            },
        )?;

        Ok(())
    })?;

    Ok(upid_str)
}

#[api(
    protected: true,
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
            options: {
                type: APTGetChangelogOptions,
                flatten: true,
            },
        },
    },
    returns: {
        description: "The Package changelog.",
        type: String,
    },
    access: {
        permission: &Permission::Privilege(&[], PRIV_SYS_MODIFY, false),
    },
)]
/// Retrieve the changelog of the specified package.
fn apt_get_changelog(options: APTGetChangelogOptions) -> Result<String, Error> {
    proxmox_apt::get_changelog(&options)
}

#[api(
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
        },
    },
    returns: {
        description: "List of more relevant packages.",
        type: Array,
        items: {
            type: APTUpdateInfo,
        },
    },
    access: {
        permission: &Permission::Privilege(&[], PRIV_SYS_AUDIT, false),
    },
)]
/// Get package information for important Proxmox Backup Server packages.
pub fn get_versions() -> Result<Vec<APTUpdateInfo>, Error> {
    const PACKAGES: &[&str] = &[
        "ifupdown2",
        //"libjs-extjs",
        //"proxmox-backup-docs",
        //"proxmox-backup-client",
        //"proxmox-mail-forward",
        //"proxmox-mini-journalreader",
        //"proxmox-offline-mirror-helper",
        //"proxmox-widget-toolkit",
        "pve-xtermjs",
        //"smartmontools",
        //"zfsutils-linux",
    ];
    //let version = pbs_buildcfg::PROXMOX_PKG_VERSION;
    //let release = pbs_buildcfg::PROXMOX_PKG_RELEASE;
    let running_daemon_version = format!("running version: {}.{}", 1, 1);

    proxmox_apt::get_package_versions(
        "proxmox-datacenter",
        "proxmox-datacenter-manager",
        &running_daemon_version,
        PACKAGES,
    )
}

#[api(
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
        },
    },
    returns: {
        type: APTRepositoriesResult,
    },
    access: {
        permission: &Permission::Privilege(&[], PRIV_SYS_AUDIT, false),
    },
)]
/// Get APT repository information.
pub fn get_repositories() -> Result<APTRepositoriesResult, Error> {
    proxmox_apt::list_repositories("pdm")
}

#[api(
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
            handle: {
                type: APTRepositoryHandle,
            },
            digest: {
                type: ConfigDigest,
                optional: true,
            },
        },
    },
    protected: true,
    access: {
        permission: &Permission::Privilege(&[], PRIV_SYS_MODIFY, false),
    },
)]
/// Add the repository identified by the `handle`.
/// If the repository is already configured, it will be set to enabled.
///
/// The `digest` parameter asserts that the configuration has not been modified.
pub fn add_repository(
    handle: APTRepositoryHandle,
    digest: Option<ConfigDigest>,
) -> Result<(), Error> {
    proxmox_apt::add_repository_handle("pdm", handle, digest)
}

#[api(
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
            path: {
                description: "Path to the containing file.",
                type: String,
            },
            index: {
                description: "Index within the file (starting from 0).",
                type: usize,
            },
            options: {
                type: APTChangeRepositoryOptions,
                flatten: true,
            },
            digest: {
                type: ConfigDigest,
                optional: true,
            },
        },
    },
    protected: true,
    access: {
        permission: &Permission::Privilege(&[], PRIV_SYS_MODIFY, false),
    },
)]
/// Change the properties of the specified repository.
///
/// The `digest` parameter asserts that the configuration has not been modified.
pub fn change_repository(
    path: String,
    index: usize,
    options: APTChangeRepositoryOptions,
    digest: Option<ConfigDigest>,
) -> Result<(), Error> {
    proxmox_apt::change_repository(&path, index, &options, digest)
}

const SUBDIRS: SubdirMap = &[
    (
        "changelog",
        &Router::new().get(&API_METHOD_APT_GET_CHANGELOG),
    ),
    (
        "repositories",
        &Router::new()
            .get(&API_METHOD_GET_REPOSITORIES)
            .post(&API_METHOD_CHANGE_REPOSITORY)
            .put(&API_METHOD_ADD_REPOSITORY),
    ),
    (
        "update",
        &Router::new()
            .get(&API_METHOD_APT_UPDATE_AVAILABLE)
            .post(&API_METHOD_APT_UPDATE_DATABASE),
    ),
    ("versions", &Router::new().get(&API_METHOD_GET_VERSIONS)),
];

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);
