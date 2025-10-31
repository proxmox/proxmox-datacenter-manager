use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use pwt::prelude::*;
use pwt::props::StorageLocation;
use pwt::state::PersistentState;
use pwt::widget::form::{DisplayField, FormContext, Number};
use pwt::widget::InputPanel;

use proxmox_client::ApiResponseData;
use proxmox_yew_comp::EditWindow;

/// The initial 'max-age' parameter in seconds. The backend polls every 15 minutes, so to increase
/// the chance of showing some data quickly use that as max age at the very first load.
pub const INITIAL_MAX_AGE_S: u64 = 900;

/// The 'max-age' parameter in seconds for when user forces a reload. Do not use 0 as the data will
/// never be realtime anyway, with 5s we get very current data while avoiding that one or more
/// "fidgety" users put unbounded load onto the remotes.
pub const FORCE_RELOAD_MAX_AGE_S: u64 = 3;

/// The default 'max-age' parameter in seconds. The backend polls every 15 minutes, but if a user
/// has the dashboard active for a longer time it's beneficial to refresh a bit more often, forcing
/// new data twice a minute is a good compromise.
pub const DEFAULT_MAX_AGE_S: u64 = 30;

/// The default refresh interval, we poll more frequently than the default max-age to quicker show
/// any new data that was gathered either by the backend polling tasks or by a manual update
/// triggered by another user.
pub const DEFAULT_REFRESH_INTERVAL_S: u32 = 10;

/// The default hours to show for task summaries. Use 2 days to ensure that all tasks from yesterday
/// are included independent from the time a user checks the dashboard on the current day.
pub const DEFAULT_TASK_SUMMARY_HOURS: u32 = 48;

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct RefreshConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_interval: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_last_hours: Option<u32>,
}

/// Get a consistent id for use in a local storage
pub fn refresh_config_id(id: &str) -> String {
    format!("view-{id}-config")
}

pub fn create_refresh_config_edit_window(id: &str) -> EditWindow {
    let id = refresh_config_id(id);
    EditWindow::new(tr!("Refresh Configuration"))
        .submit_text(tr!("Save"))
        .loader({
            move || {
                let id = id.clone();
                let data: PersistentState<RefreshConfig> = PersistentState::new(
                    StorageLocation::local(id),
                );

                async move {
                    let data = serde_json::to_value(data.into_inner())?;
                    Ok(ApiResponseData {
                        attribs: HashMap::new(),
                        data,
                    })
                }
            }
        })
    .renderer(|_ctx: &FormContext| {
        InputPanel::new()
            .width(600)
            .padding(2)
            .with_field(
                tr!("Refresh Interval (seconds)"),
                Number::new()
                .name("refresh-interval")
                .min(5u64)
                .step(5)
                .placeholder(DEFAULT_REFRESH_INTERVAL_S.to_string()),
            )
            .with_field(
                tr!("Max Age (seconds)"),
                Number::new()
                .name("max-age")
                .min(0u64)
                .step(5)
                .placeholder(DEFAULT_MAX_AGE_S.to_string()),
            )
            .with_field(
                "",
                DisplayField::new()
                .key("max-age-explanation")
                .value(tr!("If a response from a remote is older than 'Max Age', it will be updated on the next refresh.")))
            .with_field(
                tr!("Task Summary Time Range (last hours)"),
                Number::new()
                .name("task-last-hours")
                .min(0u64)
                .placeholder(DEFAULT_TASK_SUMMARY_HOURS.to_string()),
            )
            .into()
    })
}
