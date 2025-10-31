use serde::{Deserialize, Serialize};

use pdm_api_types::remotes::RemoteType;

use crate::pve::GuestType;

#[derive(Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct ViewTemplate {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub layout: ViewLayout,
}

#[derive(Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "layout-type")]
pub enum ViewLayout {
    Rows {
        #[serde(skip_serializing_if = "Vec::is_empty")]
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
