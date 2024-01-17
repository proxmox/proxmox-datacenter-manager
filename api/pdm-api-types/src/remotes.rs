use serde::{Deserialize, Serialize};

use proxmox_schema::property_string::PropertyString;
use proxmox_schema::{
    api, ApiStringFormat, ApiType, EnumEntry, OneOfSchema, Schema, StringSchema, Updater,
};

pub const REMOTE_ID_SCHEMA: Schema = StringSchema::new("Remote ID.")
    .format(&crate::PROXMOX_SAFE_ID_FORMAT)
    .min_length(2)
    .max_length(32)
    .schema();

#[api(
    properties: {
        "fingerprint": {
            type: String,
            format: &crate::FINGERPRINT_SHA256_FORMAT,
            optional: true,
        },
    },
    default_key: "hostname",
)]
/// A node and its certificate information.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NodeUrl {
    /// The node address.
    pub hostname: String,

    /// Certificate fingerprint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

#[api]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RemoteType {
    /// A Proxmox VE node.
    Pve,
}

#[api(
    properties: {
        "nodes": {
            type: Array,
            items: {
                type: String,
                description: "A cluster node IP or hostname.",
            },
        },
    },
)]
/// A Proxmox VE cluster.
#[derive(Clone, Debug, Deserialize, Serialize, Updater)]
#[serde(rename_all = "kebab-case")]
pub struct PveRemote {
    /// An id for this cluster entry.
    #[updater(skip)]
    pub id: String,

    /// A list of cluster node addresses.
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub nodes: Vec<PropertyString<NodeUrl>>,

    /// The userid used to access this cluster.
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub userid: String,

    /// The access token's secret.
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub token: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Updater)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Remote {
    Pve(PveRemote),
}

impl ApiType for Remote {
    const API_SCHEMA: Schema = OneOfSchema::new(
        "A remote API entry",
        &(
            "type",
            false,
            &StringSchema::new("The remote type")
                .format(&ApiStringFormat::Enum(&[EnumEntry::new(
                    "pve",
                    "a Proxmox VE remote entry",
                )]))
                .schema(),
        ),
        &[("pve", &PveRemote::API_SCHEMA)],
    )
    .schema();
}

impl From<PveRemote> for Remote {
    fn from(pve: PveRemote) -> Self {
        Remote::Pve(pve)
    }
}

impl Remote {
    pub fn id(&self) -> &str {
        match self {
            Self::Pve(r) => &r.id,
        }
    }
}
