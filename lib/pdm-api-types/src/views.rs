use std::{fmt::Display, str::FromStr, sync::OnceLock};

use anyhow::bail;
use const_format::concatcp;
use serde::{Deserialize, Serialize};

use proxmox_schema::{
    api, api_types::SAFE_ID_REGEX_STR, const_regex, ApiStringFormat, ApiType, ArraySchema, Schema,
    StringSchema, Updater,
};
use proxmox_section_config::{typed::ApiSectionDataEntry, SectionConfig, SectionConfigPlugin};

use crate::{
    remotes::REMOTE_ID_SCHEMA, resource::ResourceType, PROXMOX_SAFE_ID_REGEX, VIEW_ID_SCHEMA,
};

const_regex! {
    /// Regex for matching global resource IDs
    pub GLOBAL_RESOURCE_ID_REGEX = concatcp!(r"^", SAFE_ID_REGEX_STR, r"(\/", SAFE_ID_REGEX_STR, r")+$");
}

/// Schema for filter rules.
pub const FILTER_RULE_SCHEMA: Schema = StringSchema::new("Filter rule for resources.")
    .format(&ApiStringFormat::VerifyFn(verify_filter_rule))
    .type_text(
        "resource-type:<storage|qemu|lxc|sdn-zone|datastore|node>\
            |resource-pool:<pool-name>\
            |tag:<tag-name>\
            |remote:<remote-name>\
            |resource-id:<resource-id>",
    )
    .schema();

/// Schema for list of filter rules.
pub const FILTER_RULE_LIST_SCHEMA: Schema =
    ArraySchema::new("List of filter rules.", &FILTER_RULE_SCHEMA).schema();

#[api(
    properties: {
        "id": {
            schema: VIEW_ID_SCHEMA,
        },
        "include": {
            schema: FILTER_RULE_LIST_SCHEMA,
            optional: true,
        },
        "exclude": {
            schema: FILTER_RULE_LIST_SCHEMA,
            optional: true,
        }
    }
)]
#[derive(Clone, Debug, Default, Deserialize, Serialize, Updater, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// View definition
pub struct ViewConfig {
    /// View name.
    #[updater(skip)]
    pub id: String,

    /// List of includes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub include: Vec<FilterRule>,

    /// List of excludes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub exclude: Vec<FilterRule>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// Enum for the different sections in the 'views.cfg' file.
pub enum ViewConfigEntry {
    /// 'view' section
    View(ViewConfig),
}

const VIEW_SECTION_NAME: &str = "view";

impl ApiSectionDataEntry for ViewConfigEntry {
    fn section_config() -> &'static SectionConfig {
        static CONFIG: OnceLock<SectionConfig> = OnceLock::new();

        CONFIG.get_or_init(|| {
            let mut this = SectionConfig::new(&VIEW_ID_SCHEMA);

            this.register_plugin(SectionConfigPlugin::new(
                VIEW_SECTION_NAME.into(),
                Some("id".to_string()),
                ViewConfig::API_SCHEMA.unwrap_object_schema(),
            ));
            this
        })
    }

    fn section_type(&self) -> &'static str {
        match self {
            ViewConfigEntry::View(_) => VIEW_SECTION_NAME,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
/// Filter rule for includes/excludes.
pub enum FilterRule {
    /// Match a resource type.
    ResourceType(ResourceType),
    /// Match a resource pools (for PVE guests).
    ResourcePool(String),
    /// Match a (global) resource ID, e.g. 'remote/<remote>/guest/<vmid>'.
    ResourceId(String),
    /// Match a tag (for PVE guests).
    Tag(String),
    /// Match a remote.
    Remote(String),
}

impl FromStr for FilterRule {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.split_once(':') {
            Some(("resource-type", value)) => FilterRule::ResourceType(value.parse()?),
            Some(("resource-pool", value)) => {
                if !PROXMOX_SAFE_ID_REGEX.is_match(value) {
                    bail!("invalid resource-pool value: {value}");
                }
                FilterRule::ResourcePool(value.to_string())
            }
            Some(("resource-id", value)) => {
                if !GLOBAL_RESOURCE_ID_REGEX.is_match(value) {
                    bail!("invalid resource-id value: {value}");
                }

                FilterRule::ResourceId(value.to_string())
            }
            Some(("tag", value)) => {
                if !PROXMOX_SAFE_ID_REGEX.is_match(value) {
                    bail!("invalid tag value: {value}");
                }
                FilterRule::Tag(value.to_string())
            }
            Some(("remote", value)) => {
                let _ = REMOTE_ID_SCHEMA.parse_simple_value(value)?;
                FilterRule::Remote(value.to_string())
            }
            Some((ty, _)) => bail!("invalid type: {ty}"),
            None => bail!("invalid filter rule: {s}"),
        })
    }
}

// used for serializing below, caution!
impl Display for FilterRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterRule::ResourceType(resource_type) => write!(f, "resource-type:{resource_type}"),
            FilterRule::ResourcePool(pool) => write!(f, "resource-pool:{pool}"),
            FilterRule::ResourceId(id) => write!(f, "resource-id:{id}"),
            FilterRule::Tag(tag) => write!(f, "tag:{tag}"),
            FilterRule::Remote(remote) => write!(f, "remote:{remote}"),
        }
    }
}

proxmox_serde::forward_deserialize_to_from_str!(FilterRule);
proxmox_serde::forward_serialize_to_display!(FilterRule);

fn verify_filter_rule(input: &str) -> Result<(), anyhow::Error> {
    FilterRule::from_str(input).map(|_| ())
}

#[cfg(test)]
mod test {
    use anyhow::Error;

    use crate::views::FilterRule;

    fn parse_and_check_display(input: &str) -> Result<bool, Error> {
        let rule: FilterRule = input.parse()?;

        Ok(input == format!("{rule}"))
    }

    #[test]
    fn test_filter_rule() {
        assert!(parse_and_check_display("abc").is_err());
        assert!(parse_and_check_display("abc:").is_err());

        assert!(parse_and_check_display("resource-type:").is_err());
        assert!(parse_and_check_display("resource-type:lxc").unwrap());
        assert!(parse_and_check_display("resource-type:qemu").unwrap());
        assert!(parse_and_check_display("resource-type:abc").is_err());

        assert!(parse_and_check_display("resource-pool:").is_err());
        assert!(parse_and_check_display("resource-pool:somepool").unwrap());

        assert!(parse_and_check_display("resource-id:").is_err());
        assert!(parse_and_check_display("resource-id:remote/someremote/guest/100").unwrap());
        assert!(parse_and_check_display("resource-id:remote").is_err());

        assert!(parse_and_check_display("tag:").is_err());
        assert!(parse_and_check_display("tag:sometag").unwrap());

        assert!(parse_and_check_display("remote:someremote").unwrap());
        assert!(parse_and_check_display("remote:a").is_err());
    }
}
