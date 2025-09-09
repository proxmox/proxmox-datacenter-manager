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

use proxmox_yew_comp::{http_get, EditWindow, Status};
use pwt::{
    css::{AlignItems, FlexDirection, FlexFit, FlexWrap, JustifyContent},
    prelude::*,
    props::StorageLocation,
    state::PersistentState,
    widget::{
        error_message,
        form::{DisplayField, FormContext, Number},
        Button, Column, Container, Fa, InputPanel, Panel, Row,
    },
    AsyncPool,
};

use pdm_api_types::{
    resource::{NodeStatusCount, ResourcesStatus},
    TaskStatistics,
};
use pdm_client::types::TopEntity;
use pdm_search::{Search, SearchTerm};
use proxmox_client::ApiResponseData;

use crate::{pve::GuestType, remotes::AddWizard, search_provider::get_search_provider, RemoteList};

mod top_entities;
pub use top_entities::TopEntities;

mod subscription_info;
pub use subscription_info::SubscriptionInfo;

mod remote_panel;
use remote_panel::RemotePanel;

mod guest_panel;
use guest_panel::GuestPanel;

mod status_row;
use status_row::DashboardStatusRow;

mod filtered_tasks;

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
    CreateWizard(bool),
    Reload,
    ForceReload,
    UpdateConfig(DashboardConfig),
    ConfigWindow(bool),
    Search(Search),
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
    show_wizard: bool,
    show_config_window: bool,
    _context_listener: ContextHandle<RemoteList>,
    async_pool: AsyncPool,
    config: PersistentState<DashboardConfig>,
}

impl PdmDashboard {
    fn create_title_with_icon(&self, icon: &str, title: String) -> Html {
        Row::new()
            .class(AlignItems::Center)
            .gap(2)
            .with_child(Fa::new(icon))
            .with_child(title)
            .into()
    }

    fn create_node_panel(&self, ctx: &yew::Context<Self>, icon: &str, title: String) -> Panel {
        let mut search_terms = vec![SearchTerm::new("node").category(Some("type"))];
        let (status_icon, text): (Fa, String) = match &self.status {
            Some(status) => {
                match status.pve_nodes {
                    NodeStatusCount {
                        online,
                        offline,
                        unknown,
                    } if offline > 0 => {
                        search_terms.push(SearchTerm::new("offline").category(Some("status")));
                        (
                            Status::Error.into(),
                            tr!(
                                "{0} of {1} nodes are offline",
                                offline,
                                online + offline + unknown,
                            ),
                        )
                    }
                    NodeStatusCount { unknown, .. } if unknown > 0 => {
                        search_terms.push(SearchTerm::new("unknown").category(Some("status")));
                        (
                            Status::Warning.into(),
                            tr!("{0} nodes have an unknown status", unknown),
                        )
                    }
                    // FIXME, get more detailed status about the failed remotes (name, type, error)?
                    NodeStatusCount { online, .. } if status.failed_remotes > 0 => (
                        Status::Unknown.into(),
                        tr!("{0} of an unknown number of nodes online", online),
                    ),
                    NodeStatusCount { online, .. } => {
                        (Status::Success.into(), tr!("{0} nodes online", online))
                    }
                }
            }
            None => (Status::Unknown.into(), String::new()),
        };

        let loading = self.status.is_none();
        let search = Search::with_terms(search_terms);
        Panel::new()
            .flex(1.0)
            .width(300)
            .title(self.create_title_with_icon(icon, title))
            .border(true)
            .with_child(
                Column::new()
                    .padding(4)
                    .class("pwt-pointer")
                    .onclick(ctx.link().callback({
                        let search = search.clone();
                        move |_| Msg::Search(search.clone())
                    }))
                    .onkeydown(ctx.link().batch_callback({
                        let search = search.clone();
                        move |event: KeyboardEvent| match event.key().as_str() {
                            "Enter" | " " => Some(Msg::Search(search.clone())),
                            _ => None,
                        }
                    }))
                    .class(FlexFit)
                    .class(AlignItems::Center)
                    .class(JustifyContent::Center)
                    .gap(2)
                    .with_child(if loading {
                        html! {<i class={"pwt-loading-icon"} />}
                    } else {
                        status_icon.large_4x().into()
                    })
                    .with_optional_child((!loading).then_some(text)),
            )
    }

    fn create_guest_panel(&self, guest_type: GuestType) -> Panel {
        let (icon, title, status) = match guest_type {
            GuestType::Qemu => (
                "desktop",
                tr!("Virtual Machines"),
                self.status.as_ref().map(|s| s.qemu.clone()),
            ),
            GuestType::Lxc => (
                "cubes",
                tr!("Linux Container"),
                self.status.as_ref().map(|s| s.lxc.clone()),
            ),
        };
        Panel::new()
            .flex(1.0)
            .width(300)
            .title(self.create_title_with_icon(icon, title))
            .border(true)
            .with_child(GuestPanel::new(guest_type, status))
    }

    fn create_task_summary_panel(
        &self,
        statistics: &StatisticsOptions,
        remotes: Option<u32>,
    ) -> Panel {
        let (hours, since) = Self::get_task_options(&self.config);
        let title = match remotes {
            Some(count) => tr!("Task Summary for Top {0} Remotes (Last {1}h)", count, hours),
            None => tr!("Task Summary by Category (Last {0}h)", hours),
        };
        Panel::new()
            .flex(1.0)
            .width(500)
            .border(true)
            .title(self.create_title_with_icon("list", title))
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
            .title(self.create_title_with_icon(icon, title))
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
            show_wizard: false,
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
            Msg::CreateWizard(show) => {
                self.show_wizard = show;
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
            Msg::Search(search_term) => {
                if let Some(provider) = get_search_provider(ctx) {
                    provider.search(search_term.into());
                }
                false
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
                        Panel::new()
                            .title(self.create_title_with_icon("server", tr!("Remotes")))
                            .flex(1.0)
                            //.border(true)
                            .width(300)
                            .min_height(175)
                            .with_tool(
                                Button::new(tr!("Add"))
                                    .icon_class("fa fa-plus-circle")
                                    .on_activate(ctx.link().callback(|_| Msg::CreateWizard(true))),
                            )
                            .with_child(RemotePanel::new(self.status.clone())),
                    )
                    .with_child(self.create_node_panel(
                        ctx,
                        "building",
                        tr!("Virtual Environment Nodes"),
                    ))
                    .with_child(self.create_guest_panel(GuestType::Qemu))
                    .with_child(self.create_guest_panel(GuestType::Lxc))
                    // FIXME: add PBS support
                    //.with_child(self.create_node_panel(
                    //    "building-o",
                    //    tr!("Backup Server Nodes"),
                    //    &self.status.pbs_nodes,
                    //))
                    //.with_child(
                    //    Panel::new()
                    //        .flex(1.0)
                    //        .width(300)
                    //        .title(self.create_title_with_icon(
                    //            "floppy-o",
                    //            tr!("Backup Server Datastores"),
                    //        ))
                    //        .border(true)
                    //        .with_child(if self.loading {
                    //            Column::new()
                    //                .padding(4)
                    //                .class(FlexFit)
                    //                .class(JustifyContent::Center)
                    //                .class(AlignItems::Center)
                    //                .with_child(html! {<i class={"pwt-loading-icon"} />})
                    //        } else {
                    //            Column::new()
                    //                .padding(4)
                    //                .class(FlexFit)
                    //                .class(JustifyContent::Center)
                    //                .gap(2)
                    //                // FIXME: show more detailed status (usage?)
                    //                .with_child(
                    //                    Row::new()
                    //                        .gap(2)
                    //                        .with_child(
                    //                            StorageState::Available.to_fa_icon().fixed_width(),
                    //                        )
                    //                        .with_child(tr!("available"))
                    //                        .with_flex_spacer()
                    //                        .with_child(
                    //                            Container::from_tag("span").with_child(
                    //                                self.status.pbs_datastores.available,
                    //                            ),
                    //                        ),
                    //                )
                    //                .with_optional_child(
                    //                    (self.status.pbs_datastores.unknown > 0).then_some(
                    //                        Row::new()
                    //                            .gap(2)
                    //                            .with_child(
                    //                                StorageState::Unknown
                    //                                    .to_fa_icon()
                    //                                    .fixed_width(),
                    //                            )
                    //                            .with_child(tr!("unknown"))
                    //                            .with_flex_spacer()
                    //                            .with_child(
                    //                                Container::from_tag("span").with_child(
                    //                                    self.status.pbs_datastores.unknown,
                    //                                ),
                    //                            ),
                    //                    ),
                    //                )
                    //        }),
                    //)
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
                    .with_child(self.create_task_summary_panel(&self.statistics, Some(5))),
            );

        Panel::new()
            .class(FlexFit)
            .with_child(content)
            // FIXME: make pbs also addable?
            .with_optional_child(
                self.show_wizard.then_some(
                    AddWizard::new(pdm_api_types::remotes::RemoteType::Pve)
                        .on_close(ctx.link().callback(|_| Msg::CreateWizard(false)))
                        .on_submit(move |ctx| {
                            crate::remotes::create_remote(
                                ctx,
                                pdm_api_types::remotes::RemoteType::Pve,
                            )
                        }),
                ),
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
