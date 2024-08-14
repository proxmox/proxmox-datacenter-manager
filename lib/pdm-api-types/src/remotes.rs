use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use proxmox_schema::property_string::PropertyString;
use proxmox_schema::{api, ApiType, Schema, StringSchema, Updater};
use proxmox_section_config::typed::ApiSectionDataEntry;
use proxmox_section_config::{SectionConfig, SectionConfigPlugin};

use crate::Authid;

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
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct NodeUrl {
    /// The node address.
    pub hostname: String,

    /// Certificate fingerprint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

#[api]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum RemoteType {
    /// A Proxmox VE node.
    Pve,
    /// A Proxmox Backup Server node.
    Pbs,
}

serde_plain::derive_display_from_serialize!(RemoteType);
serde_plain::derive_fromstr_from_deserialize!(RemoteType);

#[api(
    properties: {
        "id": { schema: REMOTE_ID_SCHEMA },
        "type": { type: RemoteType },
        "nodes": {
            type: Array,
            items: {
                type: String,
                description: "A cluster node IP or hostname.",
            },
        },
    },
)]
/// The information required to connect to a remote instance.
#[derive(Clone, Debug, Deserialize, Serialize, Updater, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct Remote {
    #[serde(rename = "type")]
    #[updater(skip)]
    pub ty: RemoteType,

    /// An id for this entry.
    #[updater(skip)]
    pub id: String,

    /// A list of cluster node addresses.
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub nodes: Vec<PropertyString<NodeUrl>>,

    /// The auth id used to access this cluster.
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub authid: Authid,

    /// The access token's secret.
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub token: String,
}

impl ApiSectionDataEntry for Remote {
    const INTERNALLY_TAGGED: Option<&'static str> = Some("type");
    const SECION_CONFIG_USES_TYPE_KEY: bool = true;

    /// Get the `SectionConfig` configuration for this enum.
    fn section_config() -> &'static SectionConfig {
        static CONFIG: OnceLock<SectionConfig> = OnceLock::new();

        CONFIG.get_or_init(|| {
            let mut this = SectionConfig::new(&REMOTE_ID_SCHEMA).with_type_key("type");
            for ty in ["pve", "pbs"] {
                this.register_plugin(SectionConfigPlugin::new(
                    ty.to_string(),
                    Some("id".to_string()),
                    Remote::API_SCHEMA.unwrap_object_schema(),
                ));
            }
            this
        })
    }

    /// Maps an enum value to its type name.
    fn section_type(&self) -> &'static str {
        match self.ty {
            RemoteType::Pve => "pve",
            RemoteType::Pbs => "pbs",
        }
    }
}
