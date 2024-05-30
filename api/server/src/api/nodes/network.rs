use anyhow::Error;
use serde_json::{to_value, Value};

use proxmox_router::{ApiMethod, Permission, Router, RpcEnvironment};
use proxmox_schema::api;

use pdm_api_types::{Authid, ConfigDigest, NODE_SCHEMA, PRIV_SYS_AUDIT, PRIV_SYS_MODIFY};

use proxmox_system_management_api::network::{
    self, DeletableInterfaceProperty, Interface, InterfaceUpdater, NETWORK_INTERFACE_NAME_SCHEMA,
};

use proxmox_rest_server::WorkerTask;

#[api(
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
        },
    },
    returns: {
        description: "List network devices (with config digest).",
        type: Array,
        items: {
            type: Interface,
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "network", "interfaces"], PRIV_SYS_AUDIT, false),
    },
)]
/// List all network devices
pub fn list_network_devices(
    _param: Value,
    _info: &ApiMethod,
    rpcenv: &mut dyn RpcEnvironment,
) -> Result<Value, Error> {
    let (config, digest) = network::config()?;
    rpcenv["digest"] = digest.to_string().into();

    let mut list = Vec::new();

    for (iface, interface) in config.interfaces.iter() {
        if iface == "lo" {
            continue;
        } // do not list lo
        let mut item: Value = to_value(interface)?;
        item["digest"] = digest.to_string().into();
        item["iface"] = iface.to_string().into();
        list.push(item);
    }

    let diff = network::changes()?;
    if !diff.is_empty() {
        rpcenv["changes"] = diff.into();
    }

    Ok(list.into())
}

#[api(
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
            iface: {
                schema: NETWORK_INTERFACE_NAME_SCHEMA,
            },
        },
    },
    returns: { type: Interface },
    access: {
        permission: &Permission::Privilege(&["system", "network", "interfaces", "{name}"], PRIV_SYS_AUDIT, false),
    },
)]
/// Read a network interface configuration.
pub fn read_interface(iface: String, rpcenv: &mut dyn RpcEnvironment) -> Result<Value, Error> {
    let (config, digest) = network::config()?;
    rpcenv["digest"] = digest.to_string().into();

    let interface = config.lookup(&iface)?;

    let mut data: Value = to_value(interface)?;
    data["digest"] = digest.to_string().into();

    Ok(data)
}

#[api(
    protected: true,
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
            iface: {
                schema: NETWORK_INTERFACE_NAME_SCHEMA,
            },
            config: {
                type: InterfaceUpdater,
                flatten: true,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "network", "interfaces", "{iface}"], PRIV_SYS_MODIFY, false),
    },
)]
/// Create network interface configuration.
#[allow(clippy::too_many_arguments)]
pub fn create_interface(iface: String, config: InterfaceUpdater) -> Result<(), Error> {
    network::create_interface(iface, config)
}

#[api(
    protected: true,
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
            iface: {
                schema: NETWORK_INTERFACE_NAME_SCHEMA,
            },
            update: {
                type: InterfaceUpdater,
                flatten: true,
            },
            delete: {
                description: "List of properties to delete.",
                type: Array,
                optional: true,
                items: {
                    type: DeletableInterfaceProperty,
                }
            },
            digest: {
                type: ConfigDigest,
                optional: true,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "network", "interfaces", "{iface}"], PRIV_SYS_MODIFY, false),
    },
)]
/// Update network interface config.
pub fn update_interface(
    iface: String,
    update: InterfaceUpdater,
    delete: Option<Vec<DeletableInterfaceProperty>>,
    digest: Option<ConfigDigest>,
) -> Result<(), Error> {
    network::update_interface(iface, update, delete, digest)
}

#[api(
    protected: true,
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
            iface: {
                schema: NETWORK_INTERFACE_NAME_SCHEMA,
            },
            digest: {
                type: ConfigDigest,
                optional: true,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "network", "interfaces", "{iface}"], PRIV_SYS_MODIFY, false),
    },
)]
/// Remove network interface configuration.
pub fn delete_interface(iface: String, digest: Option<ConfigDigest>) -> Result<(), Error> {
    let _lock = network::lock_config()?;

    let (mut network_config, expected_digest) = network::config()?;

    expected_digest.detect_modification(digest.as_ref())?;

    let _interface = network_config.lookup(&iface)?; // check if interface exists

    network_config.interfaces.remove(&iface);

    network::save_config(&network_config)?;

    Ok(())
}

#[api(
    protected: true,
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "network", "interfaces"], PRIV_SYS_MODIFY, false),
    },
)]
/// Reload network configuration (requires ifupdown2).
pub async fn reload_network_config(rpcenv: &mut dyn RpcEnvironment) -> Result<String, Error> {
    network::assert_ifupdown2_installed()?;

    let auth_id: Authid = rpcenv.get_auth_id().unwrap().parse()?;

    let upid_str = WorkerTask::spawn(
        "srvreload",
        Some(String::from("networking")),
        auth_id.to_string(),
        true,
        |_worker| async {
            let _ = std::fs::rename(
                network::NETWORK_INTERFACES_NEW_FILENAME,
                network::NETWORK_INTERFACES_FILENAME,
            );

            network::network_reload()?;
            Ok(())
        },
    )?;

    Ok(upid_str)
}

#[api(
    protected: true,
    input: {
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "network", "interfaces"], PRIV_SYS_MODIFY, false),
    },
)]
/// Revert network configuration (rm /etc/network/interfaces.new).
pub fn revert_network_config() -> Result<(), Error> {
    let _ = std::fs::remove_file(network::NETWORK_INTERFACES_NEW_FILENAME);

    Ok(())
}

const ITEM_ROUTER: Router = Router::new()
    .get(&API_METHOD_READ_INTERFACE)
    .put(&API_METHOD_UPDATE_INTERFACE)
    .delete(&API_METHOD_DELETE_INTERFACE);

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_LIST_NETWORK_DEVICES)
    .put(&API_METHOD_RELOAD_NETWORK_CONFIG)
    .post(&API_METHOD_CREATE_INTERFACE)
    .delete(&API_METHOD_REVERT_NETWORK_CONFIG)
    .match_all("iface", &ITEM_ROUTER);
