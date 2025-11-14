use std::rc::Rc;

use anyhow::Error;
use futures::join;
use js_sys::Date;
use pwt::widget::Dialog;
use serde_json::json;
use yew::virtual_dom::{VComp, VNode};

use proxmox_yew_comp::http_get;
use pwt::css;
use pwt::prelude::*;
use pwt::props::StorageLocation;
use pwt::state::{PersistentState, SharedState};
use pwt::widget::{error_message, form::FormContext, Column, Container, Progress, Row};
use pwt::AsyncPool;

use crate::dashboard::refresh_config_edit::{
    refresh_config_id, RefreshConfig, DEFAULT_MAX_AGE_S, DEFAULT_REFRESH_INTERVAL_S,
    FORCE_RELOAD_MAX_AGE_S, INITIAL_MAX_AGE_S,
};
use crate::dashboard::subscription_info::create_subscriptions_dialog;
use crate::dashboard::tasks::get_task_options;
use crate::dashboard::types::RowWidget;
use crate::dashboard::types::{TaskSummaryGrouping, ViewLayout, ViewTemplate, WidgetType};
use crate::dashboard::{
    create_guest_panel, create_node_panel, create_pbs_datastores_panel,
    create_refresh_config_edit_window, create_remote_panel, create_sdn_panel,
    create_subscription_panel, create_task_summary_panel, create_top_entities_panel,
    DashboardStatusRow,
};
use crate::remotes::AddWizard;
use crate::{pdm_client, LoadResult};

use pdm_api_types::remotes::RemoteType;
use pdm_api_types::resource::ResourcesStatus;
use pdm_api_types::subscription::RemoteSubscriptions;
use pdm_api_types::TaskStatistics;
use pdm_client::types::TopEntities;

mod row_view;
pub use row_view::RowView;

#[derive(Properties, PartialEq)]
pub struct View {
    view: AttrValue,
}

impl From<View> for VNode {
    fn from(val: View) -> Self {
        let comp = VComp::new::<ViewComp>(Rc::new(val), None);
        VNode::from(comp)
    }
}

impl View {
    pub fn new(view: impl Into<AttrValue>) -> Self {
        Self { view: view.into() }
    }
}

pub enum LoadingResult {
    Resources(Result<ResourcesStatus, Error>),
    TopEntities(Result<pdm_client::types::TopEntities, proxmox_client::Error>),
    TaskStatistics(Result<TaskStatistics, Error>),
    SubscriptionInfo(Result<Vec<RemoteSubscriptions>, Error>),
    All,
}

pub enum Msg {
    ViewTemplateLoaded(Result<ViewTemplate, Error>),
    LoadingResult(LoadingResult),
    CreateWizard(Option<RemoteType>),
    Reload(bool),       // force
    ConfigWindow(bool), // show
    UpdateConfig(RefreshConfig),
    ShowSubscriptionsDialog(Option<Dialog>),
}

struct ViewComp {
    template: LoadResult<ViewTemplate, Error>,

    // various api call results
    status: SharedState<LoadResult<ResourcesStatus, Error>>,
    top_entities: SharedState<LoadResult<TopEntities, proxmox_client::Error>>,
    statistics: SharedState<LoadResult<TaskStatistics, Error>>,
    subscriptions: SharedState<LoadResult<Vec<RemoteSubscriptions>, Error>>,

    refresh_config: PersistentState<RefreshConfig>,

    async_pool: AsyncPool,
    loading: bool,
    load_finished_time: Option<f64>,
    show_config_window: bool,
    show_create_wizard: Option<RemoteType>,
    subscriptions_dialog: Option<Dialog>,
}

fn render_widget(
    link: yew::html::Scope<ViewComp>,
    item: &RowWidget,
    status: SharedState<LoadResult<ResourcesStatus, Error>>,
    subscriptions: SharedState<LoadResult<Vec<RemoteSubscriptions>, Error>>,
    top_entities: SharedState<LoadResult<TopEntities, proxmox_client::Error>>,
    statistics: SharedState<LoadResult<TaskStatistics, Error>>,
    refresh_config: RefreshConfig,
) -> Html {
    let mut widget = match &item.r#type {
        WidgetType::Nodes { remote_type } => create_node_panel(*remote_type, status),
        WidgetType::Guests { guest_type } => create_guest_panel(*guest_type, status),
        WidgetType::Remotes { show_wizard } => create_remote_panel(
            status,
            show_wizard.then_some(link.callback(|_| Msg::CreateWizard(Some(RemoteType::Pve)))),
            show_wizard.then_some(link.callback(|_| Msg::CreateWizard(Some(RemoteType::Pbs)))),
        ),
        WidgetType::PbsDatastores => create_pbs_datastores_panel(status),
        WidgetType::Subscription => create_subscription_panel(
            subscriptions.clone(),
            link.clone().callback(move |_| {
                let dialog = create_subscriptions_dialog(
                    subscriptions.clone(),
                    link.callback(|_| Msg::ShowSubscriptionsDialog(None)),
                );
                Msg::ShowSubscriptionsDialog(dialog)
            }),
        ),
        WidgetType::Sdn => create_sdn_panel(status),
        WidgetType::Leaderboard { leaderboard_type } => {
            create_top_entities_panel(top_entities, *leaderboard_type)
        }
        WidgetType::TaskSummary { grouping } => {
            let remotes = match grouping {
                TaskSummaryGrouping::Category => None,
                TaskSummaryGrouping::Remote => Some(5),
            };
            let (hours, since) = get_task_options(refresh_config.task_last_hours);
            create_task_summary_panel(statistics, remotes, hours, since)
        }
    };

    if let Some(title) = &item.title {
        widget.set_title(title.clone());
    }

    widget.border(false).class(css::FlexFit).into()
}

impl ViewComp {
    fn reload(&mut self, ctx: &yew::Context<Self>) {
        let max_age = if self.load_finished_time.is_some() {
            self.refresh_config.max_age.unwrap_or(DEFAULT_MAX_AGE_S)
        } else {
            INITIAL_MAX_AGE_S
        };
        self.do_reload(ctx, max_age)
    }

    fn do_reload(&mut self, ctx: &yew::Context<Self>, max_age: u64) {
        if let Some(data) = self.template.data.as_ref() {
            let link = ctx.link().clone();
            let (_, since) = get_task_options(self.refresh_config.task_last_hours);
            let (status, top_entities, tasks) = required_api_calls(&data.layout);

            self.loading = true;
            self.async_pool.spawn(async move {
                let status_future = async {
                    if status {
                        let res =
                            http_get("/resources/status", Some(json!({"max-age": max_age}))).await;
                        link.send_message(Msg::LoadingResult(LoadingResult::Resources(res)));
                    }
                };

                let entities_future = async {
                    if top_entities {
                        let client: pdm_client::PdmClient<Rc<proxmox_yew_comp::HttpClientWasm>> =
                            pdm_client();
                        let res = client.get_top_entities(None).await;
                        link.send_message(Msg::LoadingResult(LoadingResult::TopEntities(res)));
                    }
                };

                let tasks_future = async {
                    if tasks {
                        let params = Some(json!({
                            "since": since,
                            "limit": 0,
                        }));
                        let res = http_get("/remote-tasks/statistics", params).await;
                        link.send_message(Msg::LoadingResult(LoadingResult::TaskStatistics(res)));
                    }
                };

                let subs_future = async {
                    let res = http_get("/resources/subscription?verbose=true", None).await;
                    link.send_message(Msg::LoadingResult(LoadingResult::SubscriptionInfo(res)));
                };

                join!(status_future, entities_future, tasks_future, subs_future);
                link.send_message(Msg::LoadingResult(LoadingResult::All));
            });
        } else {
            ctx.link()
                .send_message(Msg::LoadingResult(LoadingResult::All));
        }
    }
}

// returns which api calls are required: status, top_entities, task statistics
fn required_api_calls(layout: &ViewLayout) -> (bool, bool, bool) {
    let mut status = false;
    let mut top_entities = false;
    let mut task_statistics = false;
    match layout {
        ViewLayout::Rows { rows } => {
            for row in rows {
                for item in row {
                    match item.r#type {
                        WidgetType::Nodes { .. }
                        | WidgetType::Guests { .. }
                        | WidgetType::Remotes { .. }
                        | WidgetType::Sdn
                        | WidgetType::PbsDatastores => {
                            status = true;
                        }
                        WidgetType::Subscription => {
                            // panel does it itself, it's always required anyway
                        }
                        WidgetType::Leaderboard { .. } => top_entities = true,
                        WidgetType::TaskSummary { .. } => task_statistics = true,
                    }
                }
            }
        }
    }

    (status, top_entities, task_statistics)
}

fn has_sub_panel(layout: Option<&ViewTemplate>) -> bool {
    match layout.map(|template| &template.layout) {
        Some(ViewLayout::Rows { rows }) => {
            for row in rows {
                for item in row {
                    if item.r#type == WidgetType::Subscription {
                        return true;
                    }
                }
            }
        }
        None => {}
    }

    false
}

impl Component for ViewComp {
    type Message = Msg;
    type Properties = View;

    fn create(ctx: &yew::Context<Self>) -> Self {
        let refresh_config: PersistentState<RefreshConfig> = PersistentState::new(
            StorageLocation::local(refresh_config_id(ctx.props().view.as_str())),
        );

        let async_pool = AsyncPool::new();
        async_pool.send_future(ctx.link().clone(), async move {
            Msg::ViewTemplateLoaded(load_template().await)
        });

        Self {
            template: LoadResult::new(),
            async_pool,

            status: SharedState::new(LoadResult::new()),
            top_entities: SharedState::new(LoadResult::new()),
            statistics: SharedState::new(LoadResult::new()),
            subscriptions: SharedState::new(LoadResult::new()),

            refresh_config,
            load_finished_time: None,
            loading: true,
            show_config_window: false,
            show_create_wizard: None,
            subscriptions_dialog: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::ViewTemplateLoaded(view_template) => {
                self.template.update(view_template);
                self.reload(ctx);
            }
            Msg::LoadingResult(loading_result) => match loading_result {
                LoadingResult::Resources(status) => self.status.write().update(status),
                LoadingResult::TopEntities(top_entities) => {
                    self.top_entities.write().update(top_entities)
                }
                LoadingResult::TaskStatistics(task_statistics) => {
                    self.statistics.write().update(task_statistics)
                }
                LoadingResult::SubscriptionInfo(subscriptions) => {
                    self.subscriptions.write().update(subscriptions);
                }
                LoadingResult::All => {
                    self.loading = false;
                    if self.load_finished_time.is_none() {
                        // immediately trigger a "normal" reload after the first load with the
                        // configured or default max-age to ensure users sees more current data.
                        ctx.link().send_message(Msg::Reload(false));
                    }
                    self.load_finished_time = Some(Date::now() / 1000.0);
                }
            },
            Msg::CreateWizard(remote_type) => {
                self.show_create_wizard = remote_type;
            }
            Msg::Reload(force) => {
                if force {
                    self.do_reload(ctx, FORCE_RELOAD_MAX_AGE_S);
                } else {
                    self.reload(ctx);
                }
            }

            Msg::ConfigWindow(show) => {
                self.show_config_window = show;
            }
            Msg::UpdateConfig(dashboard_config) => {
                let (old_hours, _) = get_task_options(self.refresh_config.task_last_hours);
                self.refresh_config.update(dashboard_config);
                let (new_hours, _) = get_task_options(self.refresh_config.task_last_hours);

                if old_hours != new_hours {
                    self.reload(ctx);
                }

                self.show_config_window = false;
            }
            Msg::ShowSubscriptionsDialog(dialog) => {
                self.subscriptions_dialog = dialog;
            }
        }
        true
    }

    fn changed(&mut self, ctx: &Context<Self>, _old_props: &Self::Properties) -> bool {
        self.async_pool = AsyncPool::new();
        self.load_finished_time = None;
        self.async_pool.send_future(ctx.link().clone(), async move {
            Msg::ViewTemplateLoaded(load_template().await)
        });
        true
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        if !self.template.has_data() {
            return Progress::new().into();
        }
        let mut view = Column::new().class(css::FlexFit).with_child(
            Container::new()
                .class("pwt-content-spacer-padding")
                .class("pwt-content-spacer-colors")
                .class("pwt-default-colors")
                .with_child(DashboardStatusRow::new(
                    self.load_finished_time,
                    self.refresh_config
                        .refresh_interval
                        .unwrap_or(DEFAULT_REFRESH_INTERVAL_S),
                    ctx.link().callback(Msg::Reload),
                    ctx.link().callback(|_| Msg::ConfigWindow(true)),
                )),
        );
        if !has_sub_panel(self.template.data.as_ref()) {
            let subs = self.subscriptions.clone();
            let link = ctx.link().clone();
            view.add_child(Row::new().class("pwt-content-spacer").with_child(
                create_subscription_panel(
                    subs.clone(),
                    link.clone().callback(move |_| {
                        let on_dialog_close = link.callback(|_| Msg::ShowSubscriptionsDialog(None));
                        let dialog = create_subscriptions_dialog(subs.clone(), on_dialog_close);
                        Msg::ShowSubscriptionsDialog(dialog)
                    }),
                ),
            ));
        }
        match self.template.data.as_ref().map(|template| &template.layout) {
            Some(ViewLayout::Rows { rows }) => {
                view.add_child(RowView::new(rows.clone(), {
                    let link = ctx.link().clone();
                    let status = self.status.clone();
                    let subscriptions = self.subscriptions.clone();
                    let top_entities = self.top_entities.clone();
                    let statistics = self.statistics.clone();
                    let refresh_config = self.refresh_config.clone();
                    move |widget: &RowWidget| {
                        render_widget(
                            link.clone(),
                            widget,
                            status.clone(),
                            subscriptions.clone(),
                            top_entities.clone(),
                            statistics.clone(),
                            refresh_config.clone(),
                        )
                    }
                }));
            }
            None => {}
        }
        // fill remaining space
        view.add_child(
            Container::new()
                .class(css::Flex::Fill)
                .class("pwt-content-spacer"),
        );
        view.add_optional_child(
            self.template
                .error
                .as_ref()
                .map(|err| error_message(&err.to_string())),
        );
        view.add_optional_child(
            self.show_config_window.then_some(
                create_refresh_config_edit_window(&ctx.props().view)
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
        );
        view.add_optional_child(self.show_create_wizard.map(|remote_type| {
            AddWizard::new(remote_type)
                .on_close(ctx.link().callback(|_| Msg::CreateWizard(None)))
                .on_submit(move |ctx| crate::remotes::create_remote(ctx, remote_type))
        }));

        view.add_optional_child(self.subscriptions_dialog.clone());

        view.into()
    }
}

async fn load_template() -> Result<ViewTemplate, Error> {
    // FIXME: load template from api

    let view_str = "
        {
          \"description\": \"some description\",
          \"layout\": {
            \"layout-type\": \"rows\",
            \"rows\": [
              [
                {
                  \"flex\": 3.0,
                  \"widget-type\": \"remotes\",
                  \"show-wizard\": true
                },
                {
                  \"flex\": 3.0,
                  \"widget-type\": \"nodes\",
                  \"remote-type\": \"pve\"
                },
                {
                  \"flex\": 3.0,
                  \"widget-type\": \"guests\",
                  \"guest-type\": \"qemu\"
                },
                {
                  \"flex\": 3.0,
                  \"widget-type\": \"nodes\",
                  \"remote-type\": \"pbs\"
                },
                {
                  \"flex\": 3.0,
                  \"widget-type\": \"guests\",
                  \"guest-type\": \"lxc\"
                },
                {
                  \"flex\": 3.0,
                  \"widget-type\": \"pbs-datastores\"
                },
                {
                  \"flex\": 5.0,
                  \"widget-type\": \"subscription\"
                }
              ],
              [
                {
                  \"widget-type\": \"leaderboard\",
                  \"leaderboard-type\": \"guest-cpu\"
                },
                {
                  \"widget-type\": \"leaderboard\",
                  \"leaderboard-type\": \"node-cpu\"
                },
                {
                  \"widget-type\": \"leaderboard\",
                  \"leaderboard-type\": \"node-memory\"
                }
              ],
              [
                {
                  \"flex\": 5.0,
                  \"widget-type\": \"task-summary\",
                  \"grouping\": \"category\",
                  \"sorting\": \"default\"
                },
                {
                  \"flex\": 5.0,
                  \"widget-type\": \"task-summary\",
                  \"grouping\": \"remote\",
                  \"sorting\": \"failed-tasks\"
                },
                {
                  \"flex\": 2.0,
                  \"widget-type\": \"sdn\"
                }
              ]
            ]
          }
        }
    ";

    let template: ViewTemplate = serde_json::from_str(view_str)?;
    Ok(template)
}
