use anyhow::Error;
use serde_json::Value;

use proxmox_router::{ApiMethod, Permission, Router, RpcEnvironment};
use proxmox_schema::api;

use pdm_api_types::{ConfigDigest, NODE_SCHEMA, PRIV_SYS_AUDIT, PRIV_SYS_MODIFY};

use proxmox_dns_api::{DeletableResolvConfProperty, ResolvConf, ResolvConfWithDigest};

#[api(
    protected: true,
    input: {
        description: "Update DNS settings.",
        properties: {
            node: {
                schema: NODE_SCHEMA,
            },
            update: {
                type: ResolvConf,
                flatten: true,
            },
            delete: {
                description: "List of properties to delete.",
                type: Array,
                optional: true,
                items: {
                    type: DeletableResolvConfProperty,
                }
            },
            digest: {
                type: ConfigDigest,
                optional: true,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["system", "network", "dns"], PRIV_SYS_MODIFY, false),
    }
)]
/// Update DNS settings
pub fn update_dns(
    update: ResolvConf,
    delete: Option<Vec<DeletableResolvConfProperty>>,
    digest: Option<ConfigDigest>,
) -> Result<(), Error> {
    proxmox_dns_api::update_dns(update, delete, digest)
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
        type: ResolvConfWithDigest,
    },
    access: {
        permission: &Permission::Privilege(&["system", "network", "dns"], PRIV_SYS_AUDIT, false),
    }
)]
/// Read DNS settings.
pub fn get_dns(
    _param: Value,
    _info: &ApiMethod,
    _rpcenv: &mut dyn RpcEnvironment,
) -> Result<ResolvConfWithDigest, Error> {
    proxmox_dns_api::read_etc_resolv_conf(None)
}

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_GET_DNS)
    .put(&API_METHOD_UPDATE_DNS);
