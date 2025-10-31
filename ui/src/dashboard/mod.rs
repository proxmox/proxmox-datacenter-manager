use std::rc::Rc;

use anyhow::Error;
use futures::join;
use js_sys::Date;
use serde_json::json;
use yew::{
    virtual_dom::{VComp, VNode},
    Component,
};

use proxmox_yew_comp::http_get;
use pwt::{
    css::{AlignItems, FlexDirection, FlexFit, FlexWrap, JustifyContent},
    prelude::*,
    props::StorageLocation,
    state::PersistentState,
    widget::{form::FormContext, Column, Container, Fa, Panel, Row},
    AsyncPool,
};

use pdm_api_types::{remotes::RemoteType, resource::ResourcesStatus, TaskStatistics};

use crate::{pve::GuestType, remotes::AddWizard};

mod top_entities;
pub use top_entities::create_top_entities_panel;

mod subscription_info;
pub use subscription_info::create_subscription_panel;

mod remote_panel;
pub use remote_panel::create_remote_panel;

mod guest_panel;
pub use guest_panel::create_guest_panel;

mod node_status_panel;
use node_status_panel::create_node_panel;

mod sdn_zone_panel;
use sdn_zone_panel::create_sdn_panel;

mod status_row;
use status_row::DashboardStatusRow;

mod filtered_tasks;

mod pbs_datastores_panel;
pub use pbs_datastores_panel::create_pbs_datastores_panel;

mod tasks;
use tasks::{create_task_summary_panel, get_task_options};

pub mod types;

pub mod view;

mod refresh_config_edit;
pub use refresh_config_edit::{
    create_refresh_config_edit_window, refresh_config_id, RefreshConfig,
};
use refresh_config_edit::{
    DEFAULT_MAX_AGE_S, DEFAULT_REFRESH_INTERVAL_S, FORCE_RELOAD_MAX_AGE_S, INITIAL_MAX_AGE_S,
};

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

pub enum LoadingResult {
    Resources(Result<ResourcesStatus, Error>),
    TopEntities(Result<pdm_client::types::TopEntities, proxmox_client::Error>),
    TaskStatistics(Result<TaskStatistics, Error>),
    All,
}

pub enum Msg {
    LoadingFinished(LoadingResult),
    CreateWizard(Option<RemoteType>),
    Reload,
    ForceReload,
    UpdateConfig(RefreshConfig),
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
    load_finished_time: Option<f64>,
    show_wizard: Option<RemoteType>,
    show_config_window: bool,
    async_pool: AsyncPool,
    config: PersistentState<RefreshConfig>,
}

impl PdmDashboard {
    fn reload(&mut self, ctx: &yew::Context<Self>) {
        let max_age = if self.load_finished_time.is_some() {
            self.config.max_age.unwrap_or(DEFAULT_MAX_AGE_S)
        } else {
            INITIAL_MAX_AGE_S
        };
        self.do_reload(ctx, max_age)
    }

    fn do_reload(&mut self, ctx: &yew::Context<Self>, max_age: u64) {
        let link = ctx.link().clone();
        let (_, since) = get_task_options(self.config.task_last_hours);

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
}

impl Component for PdmDashboard {
    type Message = Msg;
    type Properties = Dashboard;

    fn create(ctx: &yew::Context<Self>) -> Self {
        let config: PersistentState<RefreshConfig> =
            PersistentState::new(StorageLocation::local(refresh_config_id("dashboard")));
        let async_pool = AsyncPool::new();

        let mut this = Self {
            status: None,
            last_error: None,
            top_entities: None,
            last_top_entities_error: None,
            statistics: StatisticsOptions {
                data: None,
                error: None,
            },
            load_finished_time: None,
            show_wizard: None,
            show_config_window: false,
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
                        if self.load_finished_time.is_none() {
                            // immediately trigger a "normal" reload after the first load with the
                            // configured or default max-age to ensure users sees more current data.
                            ctx.link().send_message(Msg::Reload);
                        }
                        self.load_finished_time = Some(Date::now() / 1000.0);
                    }
                }
                true
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
                let (old_hours, _) = get_task_options(self.config.task_last_hours);
                self.config.update(dashboard_config);
                let (new_hours, _) = get_task_options(self.config.task_last_hours);

                if old_hours != new_hours {
                    self.reload(ctx);
                }

                self.show_config_window = false;
                true
            }
        }
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let (hours, since) = get_task_options(self.config.task_last_hours);
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
                            Some(
                                ctx.link()
                                    .callback(|_| Msg::CreateWizard(Some(RemoteType::Pve))),
                            ),
                            Some(
                                ctx.link()
                                    .callback(|_| Msg::CreateWizard(Some(RemoteType::Pbs))),
                            ),
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
                    .with_child(
                        create_pbs_datastores_panel(self.status.clone())
                            .flex(1.0)
                            .width(300),
                    )
                    .with_child(
                        create_subscription_panel()
                            .flex(1.0)
                            .width(500)
                            .min_height(150),
                    ),
            )
            .with_child(
                Container::new()
                    .class("pwt-content-spacer")
                    .class(FlexDirection::Row)
                    .class("pwt-align-content-start")
                    .padding_top(0)
                    .class(FlexWrap::Wrap)
                    //.min_height(175)
                    .with_child(
                        create_top_entities_panel(
                            self.top_entities.as_ref().map(|e| e.guest_cpu.clone()),
                            self.last_top_entities_error.as_ref(),
                            types::LeaderboardType::GuestCpu,
                        )
                        .flex(1.0)
                        .width(500)
                        .min_width(400),
                    )
                    .with_child(
                        create_top_entities_panel(
                            self.top_entities.as_ref().map(|e| e.node_cpu.clone()),
                            self.last_top_entities_error.as_ref(),
                            types::LeaderboardType::NodeCpu,
                        )
                        .flex(1.0)
                        .width(500)
                        .min_width(400),
                    )
                    .with_child(
                        create_top_entities_panel(
                            self.top_entities.as_ref().map(|e| e.node_memory.clone()),
                            self.last_top_entities_error.as_ref(),
                            types::LeaderboardType::NodeCpu,
                        )
                        .flex(1.0)
                        .width(500)
                        .min_width(400),
                    ),
            )
            .with_child(
                Container::new()
                    .class("pwt-content-spacer")
                    .class(FlexDirection::Row)
                    .class("pwt-align-content-start")
                    .style("padding-top", "0")
                    .class(pwt::css::Flex::Fill)
                    .class(FlexWrap::Wrap)
                    .with_child(
                        create_task_summary_panel(
                            self.statistics.data.clone(),
                            self.statistics.error.as_ref(),
                            None,
                            hours,
                            since,
                        )
                        .flex(1.0)
                        .width(500),
                    )
                    .with_child(
                        create_task_summary_panel(
                            self.statistics.data.clone(),
                            self.statistics.error.as_ref(),
                            Some(5),
                            hours,
                            since,
                        )
                        .flex(1.0)
                        .width(500),
                    )
                    .with_child(create_sdn_panel(self.status.clone()).flex(1.0).width(200)),
            );

        Panel::new()
            .class(FlexFit)
            .with_child(content)
            .with_optional_child(self.show_wizard.map(|remote_type| {
                AddWizard::new(remote_type)
                    .on_close(ctx.link().callback(|_| Msg::CreateWizard(None)))
                    .on_submit(move |ctx| crate::remotes::create_remote(ctx, remote_type))
            }))
            .with_optional_child(
                self.show_config_window.then_some(
                    create_refresh_config_edit_window("dashboard")
                        .on_close(ctx.link().callback(|_| Msg::ConfigWindow(false)))
                        .on_submit({
                            let link = ctx.link().clone();
                            move |ctx: FormContext| {
                                let link = link.clone();
                                async move {
                                    let data: RefreshConfig =
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
