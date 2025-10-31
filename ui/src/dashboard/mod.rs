use std::{collections::HashMap, rc::Rc};

use anyhow::Error;
use futures::join;
use js_sys::Date;
use serde::{Deserialize, Serialize};
use serde_json::json;
use yew::{
    virtual_dom::{VComp, VNode},
    Component,
};

use proxmox_yew_comp::{http_get, EditWindow};
use pwt::{
    css::{AlignItems, FlexDirection, FlexFit, FlexWrap, JustifyContent},
    prelude::*,
    props::StorageLocation,
    state::PersistentState,
    widget::{
        error_message,
        form::{DisplayField, FormContext, Number},
        Column, Container, Fa, InputPanel, Panel, Row,
    },
    AsyncPool,
};

use pdm_api_types::{remotes::RemoteType, resource::ResourcesStatus, TaskStatistics};
use pdm_client::types::TopEntity;
use proxmox_client::ApiResponseData;

use crate::{pve::GuestType, remotes::AddWizard, RemoteList};

mod top_entities;
pub use top_entities::TopEntities;

mod subscription_info;
pub use subscription_info::SubscriptionInfo;

mod remote_panel;
pub use remote_panel::create_remote_panel;

mod guest_panel;
pub use guest_panel::create_guest_panel;

mod node_status_panel;
use node_status_panel::create_node_panel;

mod sdn_zone_panel;
use sdn_zone_panel::SdnZonePanel;

mod status_row;
use status_row::DashboardStatusRow;

mod filtered_tasks;

mod pbs_datastores_panel;
use pbs_datastores_panel::PbsDatastoresPanel;

mod tasks;
use tasks::TaskSummary;

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

#[derive(Properties, PartialEq)]
pub struct Dashboard {}

impl Dashboard {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

impl Default for Dashboard {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct DashboardConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    refresh_interval: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_age: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    task_last_hours: Option<u32>,
}

pub enum LoadingResult {
    Resources(Result<ResourcesStatus, Error>),
    TopEntities(Result<pdm_client::types::TopEntities, proxmox_client::Error>),
    TaskStatistics(Result<TaskStatistics, Error>),
    All,
}

pub enum Msg {
    LoadingFinished(LoadingResult),
    RemoteListChanged(RemoteList),
    CreateWizard(Option<RemoteType>),
    Reload,
    ForceReload,
    UpdateConfig(DashboardConfig),
    ConfigWindow(bool),
}

struct StatisticsOptions {
    data: Option<TaskStatistics>,
    error: Option<Error>,
}

pub struct PdmDashboard {
    status: Option<ResourcesStatus>,
    last_error: Option<Error>,
    top_entities: Option<pdm_client::types::TopEntities>,
    last_top_entities_error: Option<proxmox_client::Error>,
    statistics: StatisticsOptions,
    loaded_once: bool,
    loading: bool,
    load_finished_time: Option<f64>,
    remote_list: RemoteList,
    show_wizard: Option<RemoteType>,
    show_config_window: bool,
    _context_listener: ContextHandle<RemoteList>,
    async_pool: AsyncPool,
    config: PersistentState<DashboardConfig>,
}

impl PdmDashboard {
    fn create_sdn_panel(&self) -> Panel {
        let sdn_zones_status = self.status.as_ref().map(|status| status.sdn_zones.clone());

        Panel::new()
            .flex(1.0)
            .width(200)
            .title(create_title_with_icon("sdn", tr!("SDN Zones")))
            .border(true)
            .with_child(SdnZonePanel::new(
                (!self.loading).then_some(sdn_zones_status).flatten(),
            ))
    }

    fn create_task_summary_panel(
        &self,
        statistics: &StatisticsOptions,
        remotes: Option<u32>,
    ) -> Panel {
        let (hours, since) = Self::get_task_options(&self.config);
        let title = match remotes {
            Some(_count) => tr!("Task Summary Sorted by Failed Tasks (Last {0}h)", hours),
            None => tr!("Task Summary by Category (Last {0}h)", hours),
        };
        Panel::new()
            .flex(1.0)
            .width(500)
            .border(true)
            .title(create_title_with_icon("list", title))
            .with_child(
                Container::new()
                    .class(FlexFit)
                    .padding(2)
                    .with_optional_child(
                        statistics
                            .data
                            .clone()
                            .map(|data| TaskSummary::new(data, since, remotes)),
                    )
                    .with_optional_child(
                        (statistics.error.is_none() && statistics.data.is_none())
                            .then_some(loading_column()),
                    )
                    .with_optional_child(
                        statistics
                            .error
                            .as_ref()
                            .map(|err| error_message(&err.to_string())),
                    ),
            )
    }

    fn create_top_entities_panel(
        &self,
        icon: &str,
        title: String,
        metrics_title: String,
        entities: Option<&Vec<TopEntity>>,
        threshold: f64,
    ) -> Panel {
        Panel::new()
            .flex(1.0)
            .width(500)
            .min_width(400)
            .border(true)
            .title(create_title_with_icon(icon, title))
            .with_optional_child(
                entities
                    .map(|entities| TopEntities::new(entities.clone(), metrics_title, threshold)),
            )
            .with_optional_child(self.top_entities.is_none().then_some(loading_column()))
            .with_optional_child(
                self.last_top_entities_error
                    .as_ref()
                    .map(|err| error_message(&err.to_string())),
            )
    }

    fn create_pbs_datastores_panel(&self) -> Panel {
        let pbs_datastores = self
            .status
            .as_ref()
            .map(|status| status.pbs_datastores.clone());

        Panel::new()
            .flex(1.0)
            .width(300)
            .title(create_title_with_icon(
                "database",
                tr!("Backup Server Datastores"),
            ))
            .border(true)
            .with_child(PbsDatastoresPanel::new(pbs_datastores))
    }

    fn reload(&mut self, ctx: &yew::Context<Self>) {
        let max_age = if self.loaded_once {
            self.config.max_age.unwrap_or(DEFAULT_MAX_AGE_S)
        } else {
            INITIAL_MAX_AGE_S
        };
        self.do_reload(ctx, max_age)
    }

    fn do_reload(&mut self, ctx: &yew::Context<Self>, max_age: u64) {
        let link = ctx.link().clone();
        let (_, since) = Self::get_task_options(&self.config);

        self.load_finished_time = None;
        self.async_pool.spawn(async move {
            let client = crate::pdm_client();

            let top_entities_future = {
                let link = link.clone();
                async move {
                    let res = client.get_top_entities().await;
                    link.send_message(Msg::LoadingFinished(LoadingResult::TopEntities(res)));
                }
            };
            let status_future = {
                let link = link.clone();
                async move {
                    let res: Result<ResourcesStatus, _> =
                        http_get("/resources/status", Some(json!({"max-age": max_age}))).await;
                    link.send_message(Msg::LoadingFinished(LoadingResult::Resources(res)));
                }
            };

            let params = Some(json!({
                "since": since,
                "limit": 0,
            }));

            // TODO replace with pdm client call
            let statistics_future = {
                let link = link.clone();
                async move {
                    let res: Result<TaskStatistics, _> =
                        http_get("/remote-tasks/statistics", params).await;
                    link.send_message(Msg::LoadingFinished(LoadingResult::TaskStatistics(res)));
                }
            };
            join!(top_entities_future, status_future, statistics_future);
            link.send_message(Msg::LoadingFinished(LoadingResult::All));
        });
    }

    fn get_task_options(config: &PersistentState<DashboardConfig>) -> (u32, i64) {
        let hours = config.task_last_hours.unwrap_or(DEFAULT_TASK_SUMMARY_HOURS);
        let since = (Date::now() / 1000.0) as i64 - (hours * 60 * 60) as i64;
        (hours, since)
    }
}

impl Component for PdmDashboard {
    type Message = Msg;
    type Properties = Dashboard;

    fn create(ctx: &yew::Context<Self>) -> Self {
        let config: PersistentState<DashboardConfig> =
            PersistentState::new(StorageLocation::local("dashboard-config"));
        let async_pool = AsyncPool::new();

        let (remote_list, _context_listener) = ctx
            .link()
            .context(ctx.link().callback(Msg::RemoteListChanged))
            .expect("No Remote list context provided");

        let mut this = Self {
            status: None,
            last_error: None,
            top_entities: None,
            last_top_entities_error: None,
            statistics: StatisticsOptions {
                data: None,
                error: None,
            },
            loaded_once: false,
            loading: true,
            load_finished_time: None,
            remote_list,
            show_wizard: None,
            show_config_window: false,
            _context_listener,
            async_pool,
            config,
        };

        this.reload(ctx);

        this
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadingFinished(res) => {
                match res {
                    LoadingResult::Resources(resources_status) => match resources_status {
                        Ok(status) => {
                            self.last_error = None;
                            self.status = Some(status);
                        }
                        Err(err) => self.last_error = Some(err),
                    },
                    LoadingResult::TopEntities(top_entities) => match top_entities {
                        Ok(data) => {
                            self.last_top_entities_error = None;
                            self.top_entities = Some(data);
                        }
                        Err(err) => self.last_top_entities_error = Some(err),
                    },

                    LoadingResult::TaskStatistics(task_statistics) => match task_statistics {
                        Ok(statistics) => {
                            self.statistics.error = None;
                            self.statistics.data = Some(statistics);
                        }
                        Err(err) => self.statistics.error = Some(err),
                    },
                    LoadingResult::All => {
                        self.loading = false;
                        if !self.loaded_once {
                            self.loaded_once = true;
                            // immediately trigger a "normal" reload after the first load with the
                            // configured or default max-age to ensure users sees more current data.
                            ctx.link().send_message(Msg::Reload);
                        }
                        self.load_finished_time = Some(Date::now() / 1000.0);
                    }
                }
                true
            }
            Msg::RemoteListChanged(remote_list) => {
                let changed = self.remote_list != remote_list;
                self.remote_list = remote_list;
                changed
            }
            Msg::CreateWizard(remote_type) => {
                self.show_wizard = remote_type;
                true
            }
            Msg::Reload => {
                self.reload(ctx);
                true
            }
            Msg::ForceReload => {
                self.do_reload(ctx, FORCE_RELOAD_MAX_AGE_S);
                true
            }
            Msg::ConfigWindow(show) => {
                self.show_config_window = show;
                true
            }
            Msg::UpdateConfig(dashboard_config) => {
                let (old_hours, _) = Self::get_task_options(&self.config);
                self.config.update(dashboard_config);
                let (new_hours, _) = Self::get_task_options(&self.config);

                if old_hours != new_hours {
                    self.reload(ctx);
                }

                self.show_config_window = false;
                true
            }
        }
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let content = Column::new()
            .class(FlexFit)
            .with_child(
                Container::new()
                    .class("pwt-content-spacer-padding")
                    .class("pwt-content-spacer-colors")
                    .style("color", "var(--pwt-color)")
                    .style("background-color", "var(--pwt-color-background)")
                    .with_child(DashboardStatusRow::new(
                        self.load_finished_time,
                        self.config
                            .refresh_interval
                            .unwrap_or(DEFAULT_REFRESH_INTERVAL_S),
                        ctx.link()
                            .callback(|force| if force { Msg::ForceReload } else { Msg::Reload }),
                        ctx.link().callback(|_| Msg::ConfigWindow(true)),
                    )),
            )
            .with_child(
                Container::new()
                    .class("pwt-content-spacer")
                    .class(FlexDirection::Row)
                    .class(FlexWrap::Wrap)
                    .padding_top(0)
                    .with_child(
                        create_remote_panel(
                            self.status.clone(),
                            ctx.link()
                                .callback(|_| Msg::CreateWizard(Some(RemoteType::Pve))),
                            ctx.link()
                                .callback(|_| Msg::CreateWizard(Some(RemoteType::Pbs))),
                        )
                        .flex(1.0)
                        .width(300)
                        .min_height(175),
                    )
                    .with_child(
                        create_node_panel(Some(RemoteType::Pve), self.status.clone())
                            .flex(1.0)
                            .width(300),
                    )
                    .with_child(
                        create_guest_panel(Some(GuestType::Qemu), self.status.clone())
                            .flex(1.0)
                            .width(300),
                    )
                    .with_child(
                        create_guest_panel(Some(GuestType::Lxc), self.status.clone())
                            .flex(1.0)
                            .width(300),
                    )
                    .with_child(
                        create_node_panel(Some(RemoteType::Pbs), self.status.clone())
                            .flex(1.0)
                            .width(300),
                    )
                    .with_child(self.create_pbs_datastores_panel())
                    .with_child(SubscriptionInfo::new()),
            )
            .with_child(
                Container::new()
                    .class("pwt-content-spacer")
                    .class(FlexDirection::Row)
                    .class("pwt-align-content-start")
                    .padding_top(0)
                    .class(FlexWrap::Wrap)
                    //.min_height(175)
                    .with_child(self.create_top_entities_panel(
                        "desktop",
                        tr!("Guests With the Highest CPU Usage"),
                        tr!("CPU usage"),
                        self.top_entities.as_ref().map(|e| &e.guest_cpu),
                        0.85,
                    ))
                    .with_child(self.create_top_entities_panel(
                        "building",
                        tr!("Nodes With the Highest CPU Usage"),
                        tr!("CPU usage"),
                        self.top_entities.as_ref().map(|e| &e.node_cpu),
                        0.85,
                    ))
                    .with_child(self.create_top_entities_panel(
                        "building",
                        tr!("Nodes With the Highest Memory Usage"),
                        tr!("Memory usage"),
                        self.top_entities.as_ref().map(|e| &e.node_memory),
                        0.95,
                    )),
            )
            .with_child(
                Container::new()
                    .class("pwt-content-spacer")
                    .class(FlexDirection::Row)
                    .class("pwt-align-content-start")
                    .style("padding-top", "0")
                    .class(pwt::css::Flex::Fill)
                    .class(FlexWrap::Wrap)
                    .with_child(self.create_task_summary_panel(&self.statistics, None))
                    .with_child(self.create_task_summary_panel(&self.statistics, Some(5)))
                    .with_child(self.create_sdn_panel()),
            );

        Panel::new()
            .class(FlexFit)
            .with_child(content)
            .with_optional_child(
                self.show_wizard.map(|remote_type| {
                    AddWizard::new(remote_type)
                        .on_close(ctx.link().callback(|_| Msg::CreateWizard(None)))
                        .on_submit(move |ctx| {
                            crate::remotes::create_remote(ctx, remote_type)
                        })
                }),
            )
            .with_optional_child(
                self.show_config_window.then_some(
                    EditWindow::new(tr!("Dashboard Configuration"))
                        .submit_text(tr!("Save"))
                        .loader({
                            || {
                                let data: PersistentState<DashboardConfig> = PersistentState::new(
                                    StorageLocation::local("dashboard-config"),
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
                        .on_close(ctx.link().callback(|_| Msg::ConfigWindow(false)))
                        .on_submit({
                            let link = ctx.link().clone();
                            move |ctx: FormContext| {
                                let link = link.clone();
                                async move {
                                    let data: DashboardConfig =
                                        serde_json::from_value(ctx.get_submit_data())?;
                                    link.send_message(Msg::UpdateConfig(data));
                                    Ok(())
                                }
                            }
                        }),
                ),
            )
            .into()
    }
}

impl From<Dashboard> for VNode {
    fn from(val: Dashboard) -> Self {
        let comp = VComp::new::<PdmDashboard>(Rc::new(val), None);
        VNode::from(comp)
    }
}

fn loading_column() -> Column {
    Column::new()
        .padding(4)
        .class(FlexFit)
        .class(JustifyContent::Center)
        .class(AlignItems::Center)
        .with_child(html! {<i class={"pwt-loading-icon"} />})
}

/// Create a consistent title component for the given title and icon
pub fn create_title_with_icon(icon: &str, title: String) -> Html {
    Row::new()
        .class(AlignItems::Center)
        .gap(2)
        .with_child(Fa::new(icon))
        .with_child(title)
        .into()
}
