use std::sync::{Arc, Mutex};

use ::serde::{Deserialize, Serialize};
use anyhow::Error;
use const_format::concatcp;
use lazy_static::lazy_static;
use openssl::sha;
use regex::Regex;
use serde_json::Value;

use pdm_api_types::IPRE_STR;

use proxmox_router::{ApiMethod, Permission, Router, RpcEnvironment};
use proxmox_schema::api;
use proxmox_sys::fs::{file_get_contents, replace_file, CreateOptions};

use pdm_api_types::{
    FIRST_DNS_SERVER_SCHEMA, NODE_SCHEMA, PRIV_SYS_AUDIT, PRIV_SYS_MODIFY,
    PROXMOX_CONFIG_DIGEST_SCHEMA, SEARCH_DOMAIN_SCHEMA, SECOND_DNS_SERVER_SCHEMA,
    THIRD_DNS_SERVER_SCHEMA,
};

static RESOLV_CONF_FN: &str = "/etc/resolv.conf";

#[api(
    properties: {
        search: {
            schema: SEARCH_DOMAIN_SCHEMA,
            optional: true,
        },
        dns1: {
            optional: true,
            schema: FIRST_DNS_SERVER_SCHEMA,
        },
        dns2: {
            optional: true,
            schema: SECOND_DNS_SERVER_SCHEMA,
        },
        dns3: {
            optional: true,
            schema: THIRD_DNS_SERVER_SCHEMA,
        },
        options: {
            description: "Other data found in the configuration file (resolv.conf).",
            optional: true,
        },

    }
)]
#[derive(Serialize, Deserialize, Default)]
/// DNS configuration from '/etc/resolv.conf'
pub struct ResolvConf {
    pub search: Option<String>,
    pub dns1: Option<String>,
    pub dns2: Option<String>,
    pub dns3: Option<String>,
    pub options: Option<String>,
}

#[api(
    properties: {
        config: {
            type: ResolvConf,
        },
        digest: {
            schema: PROXMOX_CONFIG_DIGEST_SCHEMA,
        },
    }
)]
#[derive(Serialize, Deserialize)]
/// DNS configuration with digest.
pub struct ResolvConfWithDigest {
    #[serde(flatten)]
    pub config: ResolvConf,
    pub digest: String,
}

#[api()]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
/// Deletable property name
pub enum DeletableProperty {
    /// Delete first nameserver entry
    Dns1,
    /// Delete second nameserver entry
    Dns2,
    /// Delete third nameserver entry
    Dns3,
}

pub fn read_etc_resolv_conf(expected_digest: Option<&str>) -> Result<ResolvConfWithDigest, Error> {
    let mut config = ResolvConf::default();

    let mut nscount = 0;

    let raw = file_get_contents(RESOLV_CONF_FN)?;
    let digest = sha::sha256(&raw);

    pdm_config::detect_modified_configuration_file(expected_digest, &digest)?;

    let digest = hex::encode(digest);

    let data = String::from_utf8(raw)?;

    lazy_static! {
        static ref DOMAIN_REGEX: Regex = Regex::new(r"^\s*(?:search|domain)\s+(\S+)\s*").unwrap();
        static ref SERVER_REGEX: Regex =
            Regex::new(concatcp!(r"^\s*nameserver\s+(", IPRE_STR, r")\s*")).unwrap();
    }

    let mut options = String::new();

    for line in data.lines() {
        if let Some(caps) = DOMAIN_REGEX.captures(line) {
            config.search = Some(caps[1].to_owned());
        } else if let Some(caps) = SERVER_REGEX.captures(line) {
            nscount += 1;
            if nscount > 3 {
                continue;
            };
            let nameserver = Some(caps[1].to_owned());
            match nscount {
                1 => config.dns1 = nameserver,
                2 => config.dns2 = nameserver,
                3 => config.dns3 = nameserver,
                _ => continue,
            }
        } else {
            if !options.is_empty() {
                options.push('\n');
            }
            options.push_str(line);
        }
    }

    if !options.is_empty() {
        config.options = Some(options);
    }

    Ok(ResolvConfWithDigest { config, digest })
}

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
                    type: DeletableProperty,
                }
            },
            digest: {
                schema: PROXMOX_CONFIG_DIGEST_SCHEMA,
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
    delete: Option<Vec<DeletableProperty>>,
    digest: Option<String>,
) -> Result<(), Error> {
    lazy_static! {
        static ref MUTEX: Arc<Mutex<()>> = Arc::new(Mutex::new(()));
    }

    let _guard = MUTEX.lock();

    let ResolvConfWithDigest { mut config, .. } = read_etc_resolv_conf(digest.as_deref())?;

    if let Some(delete) = delete {
        for delete_prop in delete {
            match delete_prop {
                DeletableProperty::Dns1 => {
                    config.dns1 = None;
                }
                DeletableProperty::Dns2 => {
                    config.dns2 = None;
                }
                DeletableProperty::Dns3 => {
                    config.dns3 = None;
                }
            }
        }
    }

    if update.search.is_some() {
        config.search = update.search;
    }
    if update.dns1.is_some() {
        config.dns1 = update.dns1;
    }
    if update.dns2.is_some() {
        config.dns2 = update.dns2;
    }
    if update.dns3.is_some() {
        config.dns3 = update.dns3;
    }

    let mut data = String::new();

    use std::fmt::Write as _;
    if let Some(search) = config.search {
        let _ = writeln!(data, "search {}", search);
    }

    if let Some(dns1) = config.dns1 {
        let _ = writeln!(data, "nameserver {}", dns1);
    }

    if let Some(dns2) = config.dns2 {
        let _ = writeln!(data, "nameserver {}", dns2);
    }

    if let Some(dns3) = config.dns3 {
        let _ = writeln!(data, "nameserver {}", dns3);
    }

    if let Some(options) = config.options {
        data.push_str(&options);
    }

    replace_file(RESOLV_CONF_FN, data.as_bytes(), CreateOptions::new(), true)?;

    Ok(())
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
    read_etc_resolv_conf(None)
}

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_GET_DNS)
    .put(&API_METHOD_UPDATE_DNS);
