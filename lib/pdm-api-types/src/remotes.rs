use std::sync::OnceLock;

use http::Uri;
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
/// The type of a remote entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum RemoteType {
    /// A Proxmox VE node.
    Pve,
    /// A Proxmox Backup Server node.
    Pbs,
}

impl RemoteType {
    pub fn default_port(&self) -> u16 {
        match self {
            RemoteType::Pve => 8006,
            RemoteType::Pbs => 8007,
        }
    }
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
        "web-url": {
            type: String,
            optional: true,
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

    /// Configuration for the Web UI URL link generation.
    #[updater(serde(
        default,
        with = "serde_option_uri",
        skip_serializing_if = "Option::is_none"
    ))]
    #[updater(type = "Option<Uri>")]
    #[serde(
        default,
        with = "serde_option_uri",
        skip_serializing_if = "Option::is_none"
    )]
    pub web_url: Option<Uri>,
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

/// Since `Uri` does not directly support `serde`, we turn this into using FromStr/Display.
///
/// If we want to turn this into a property string, we can use a default_key, but may need to work
/// around the fact that urls may contain `=`, so the regular property string parser might not
/// realize it's supposed to use a default key.
mod serde_option_uri {
    use std::borrow::Cow;

    use http::Uri;
    use serde::Deserialize;

    pub fn serialize<S>(uri: &Option<Uri>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match uri {
            Some(uri) => serializer.serialize_str(&uri.to_string()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Uri>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        match <Option<Cow<str>>>::deserialize(deserializer)? {
            None => Ok(None),
            Some(uri) => uri.parse::<Uri>().map(Some).map_err(D::Error::custom),
        }
    }
}
