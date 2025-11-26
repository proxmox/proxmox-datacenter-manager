use std::{fmt::Debug, fmt::Display, str::FromStr, sync::OnceLock};

use anyhow::{bail, Error};
use const_format::concatcp;
use serde::{Deserialize, Serialize};

use proxmox_schema::{
    api, api_types::SAFE_ID_REGEX_STR, const_regex, ApiStringFormat, ApiType, ArraySchema, Schema,
    StringSchema, Updater,
};
use proxmox_section_config::{typed::ApiSectionDataEntry, SectionConfig, SectionConfigPlugin};

use crate::{
    remotes::{RemoteType, REMOTE_ID_SCHEMA},
    resource::{GuestType, ResourceType},
    PROXMOX_SAFE_ID_REGEX, VIEW_ID_SCHEMA,
};

const_regex! {
    /// Regex for matching global resource IDs
    pub GLOBAL_RESOURCE_ID_REGEX = concatcp!(r"^", SAFE_ID_REGEX_STR, r"(\/", SAFE_ID_REGEX_STR, r")+$");
}

/// Schema for filter rules.
pub const FILTER_RULE_SCHEMA: Schema = StringSchema::new("Filter rule for resources.")
    .format(&ApiStringFormat::VerifyFn(verify_filter_rule))
    .type_text(
        "[exact:]resource-type=<storage|qemu|lxc|sdn-zone|datastore|node>\
            |[exact:]resource-pool=<pool-name>\
            |[exact:]tag=<tag-name>\
            |[exact:]remote=<remote-name>\
            |[exact:]resource=id:<resource-id>",
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
        },
        layout: {
            optional: true,
        },
    }
)]
#[derive(Clone, Debug, Default, Deserialize, Serialize, Updater, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// View definition
pub struct ViewConfig {
    /// View name.
    #[updater(skip)]
    pub id: String,

    /// Include all resources by default.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub include_all: Option<bool>,

    /// List of includes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub include: Vec<FilterRule>,

    /// List of excludes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub exclude: Vec<FilterRule>,

    // we can't currently describe this with the 'api' macro so save
    // it simply as a string and check it in the add/update call
    /// The configured layout, encoded as json
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[updater(serde(skip_serializing_if = "Option::is_none"))]
    pub layout: String,
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
/// Matcher for string-based values.
pub enum StringMatcher {
    Exact(String),
}

impl StringMatcher {
    /// Check if a given string matches.
    pub fn matches(&self, value: &str) -> bool {
        match self {
            StringMatcher::Exact(matched_value) => value == matched_value,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
/// Matcher for enum-based values.
pub struct EnumMatcher<T: PartialEq + Clone + Debug>(pub T);

impl<T: PartialEq + Debug + Clone> EnumMatcher<T> {
    /// Check if a given value matches.
    pub fn matches(&self, value: &T) -> bool {
        self.0 == *value
    }
}

#[derive(Clone, Debug, PartialEq)]
/// Filter rule for includes/excludes.
pub enum FilterRule {
    /// Match a resource type.
    ResourceType(EnumMatcher<ResourceType>),
    /// Match a resource pools (for PVE guests).
    ResourcePool(StringMatcher),
    /// Match a (global) resource ID, e.g. 'remote/<remote>/guest/<vmid>'.
    ResourceId(StringMatcher),
    /// Match a tag (for PVE guests).
    Tag(StringMatcher),
    /// Match a remote.
    Remote(StringMatcher),
}

impl FromStr for FilterRule {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(s) = s.strip_prefix("exact:") {
            parse_filter_rule(s)
        } else {
            parse_filter_rule(s)
        }
    }
}

fn parse_filter_rule(s: &str) -> Result<FilterRule, Error> {
    Ok(match s.split_once('=') {
        Some(("resource-type", value)) => FilterRule::ResourceType(EnumMatcher(value.parse()?)),
        Some(("resource-pool", value)) => {
            if !PROXMOX_SAFE_ID_REGEX.is_match(value) {
                bail!("invalid resource-pool value: {value}");
            }

            let val = StringMatcher::Exact(value.into());
            FilterRule::ResourcePool(val)
        }
        Some(("resource-id", value)) => {
            if !GLOBAL_RESOURCE_ID_REGEX.is_match(value) {
                bail!("invalid resource-id value: {value}");
            }

            let val = StringMatcher::Exact(value.into());
            FilterRule::ResourceId(val)
        }
        Some(("tag", value)) => {
            if !PROXMOX_SAFE_ID_REGEX.is_match(value) {
                bail!("invalid tag value: {value}");
            }
            let val = StringMatcher::Exact(value.into());
            FilterRule::Tag(val)
        }
        Some(("remote", value)) => {
            if !PROXMOX_SAFE_ID_REGEX.is_match(value) {
                let _ = REMOTE_ID_SCHEMA.parse_simple_value(value)?;
            }
            let val = StringMatcher::Exact(value.into());
            FilterRule::Remote(val)
        }
        Some((ty, _)) => bail!("invalid type: {ty}"),
        None => bail!("invalid filter rule: {s}"),
    })
}

// used for serializing below, caution!
impl Display for FilterRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterRule::ResourceType(EnumMatcher(resource_type)) => {
                write!(f, "exact:resource-type={resource_type}")
            }
            FilterRule::ResourceId(StringMatcher::Exact(value)) => {
                write!(f, "exact:resource-id={value}")
            }
            FilterRule::Tag(StringMatcher::Exact(value)) => write!(f, "exact:tag={value}"),
            FilterRule::Remote(StringMatcher::Exact(value)) => write!(f, "exact:remote={value}"),
            FilterRule::ResourcePool(StringMatcher::Exact(value)) => {
                write!(f, "exact:resource-pool={value}")
            }
        }
    }
}

proxmox_serde::forward_deserialize_to_from_str!(FilterRule);
proxmox_serde::forward_serialize_to_display!(FilterRule);

fn verify_filter_rule(input: &str) -> Result<(), anyhow::Error> {
    FilterRule::from_str(input).map(|_| ())
}

#[derive(Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct ViewTemplate {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub layout: ViewLayout,
}

#[derive(Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "layout-type")]
pub enum ViewLayout {
    Rows {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        rows: Vec<Vec<RowWidget>>,
    },
}

#[derive(Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct RowWidget {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flex: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(flatten)]
    pub r#type: WidgetType,
}

#[derive(Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "widget-type")]
pub enum WidgetType {
    #[serde(rename_all = "kebab-case")]
    Nodes {
        #[serde(skip_serializing_if = "Option::is_none")]
        remote_type: Option<RemoteType>,
    },
    #[serde(rename_all = "kebab-case")]
    Guests {
        #[serde(skip_serializing_if = "Option::is_none")]
        guest_type: Option<GuestType>,
    },
    PbsDatastores,
    #[serde(rename_all = "kebab-case")]
    Remotes {
        show_wizard: bool,
    },
    Subscription,
    Sdn,
    #[serde(rename_all = "kebab-case")]
    Leaderboard {
        leaderboard_type: LeaderboardType,
    },
    TaskSummary {
        grouping: TaskSummaryGrouping,
    },
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum LeaderboardType {
    GuestCpu,
    NodeCpu,
    NodeMemory,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum TaskSummaryGrouping {
    Category,
    Remote,
}

#[cfg(test)]
mod test {
    use anyhow::Error;

    use proxmox_section_config::typed::ApiSectionDataEntry;

    use super::*;

    fn parse_and_check_display(input: &str) -> Result<bool, Error> {
        let rule: FilterRule = input.parse()?;

        Ok(input == format!("{rule}"))
    }

    #[test]
    fn test_filter_rule() {
        assert!(parse_and_check_display("abc").is_err());
        assert!(parse_and_check_display("abc:").is_err());

        assert!(parse_and_check_display("resource-type:").is_err());
        assert!(parse_and_check_display("exact:resource-type=lxc").unwrap());
        assert!(parse_and_check_display("exact:resource-type=qemu").unwrap());
        assert!(parse_and_check_display("exact:resource-type=abc").is_err());

        assert!(parse_and_check_display("resource-pool:").is_err());
        assert!(parse_and_check_display("exact:resource-pool=somepool").unwrap());

        assert!(parse_and_check_display("resource-id:").is_err());
        assert!(parse_and_check_display("exact:resource-id=remote/someremote/guest/100").unwrap());
        assert!(parse_and_check_display("exact:resource-id=remote").is_err());

        assert!(parse_and_check_display("tag:").is_err());
        assert!(parse_and_check_display("exact:tag=sometag").unwrap());

        assert!(parse_and_check_display("exact:remote=someremote").unwrap());
        assert!(parse_and_check_display("remote:a").is_err());
    }

    #[test]
    fn config_smoke_test() {
        let config = "
view: some-view
    include-all true
    include exact:remote=someremote
    include remote=someremote
    include resource-type=qemu
    include exact:resource-type=qemu
    include resource-id=remote/someremote/guest/100
    include exact:resource-id=remote/someremote/guest/100
    include tag=sometag
    include exact:tag=sometag
    include resource-pool=somepool
    include exact:resource-pool=somepool
    exclude remote=someremote
    exclude exact:remote=someremote
    exclude resource-type=qemu
    exclude exact:resource-type=qemu
    exclude resource-id=remote/someremote/guest/100
    exclude exact:resource-id=remote/someremote/guest/100
    exclude tag=sometag
    exclude exact:tag=sometag
    exclude resource-pool=somepool
    exclude exact:resource-pool=somepool
";
        ViewConfigEntry::parse_section_config("views.cfg", config).unwrap();
    }
}
