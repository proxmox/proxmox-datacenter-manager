use anyhow::Error;
use gloo_timers::callback::Interval;
use yew::html::IntoPropValue;
use yew::{Component, Properties};
use yew_router::prelude::RouterScopeExt;
use yew_router::AnyRoute;

use pwt::prelude::*;
use pwt::state::SharedState;
use pwt::{css, AsyncPool};
use pwt::{
    css::AlignItems,
    widget::{ActionIcon, Container, Row, Tooltip},
};
use pwt_macros::{builder, widget};

use proxmox_subscription::SubscriptionStatus;
use proxmox_yew_comp::subscription_icon;
use proxmox_yew_comp::utils::render_epoch;

use pdm_api_types::subscription::PdmSubscriptionInfo;

use crate::dashboard::view::EditingMessage;
use crate::LoadResult;

#[widget(comp=PdmDashboardStatusRow)]
#[derive(Properties, PartialEq, Clone)]
#[builder]
pub struct DashboardStatusRow {
    last_refresh: Option<f64>,
    reload_interval_s: u32,

    on_reload: Callback<bool>,

    on_settings_click: Callback<()>,

    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    /// If added, shows a edit/finish/cancel button
    editing_state: Option<SharedState<Vec<EditingMessage>>>,
}

impl DashboardStatusRow {
    pub fn new(
        last_refresh: Option<f64>,
        reload_interval_s: u32,
        on_reload: impl Into<Callback<bool>>,
        on_settings_click: impl Into<Callback<()>>,
    ) -> Self {
        yew::props!(Self {
            last_refresh,
            reload_interval_s,
            on_reload: on_reload.into(),
            on_settings_click: on_settings_click.into(),
        })
    }
}

pub enum Msg {
    /// The bool denotes if the reload comes from the click or the timer.
    Reload(bool),
    SubscriptionInfoLoaded(Result<PdmSubscriptionInfo, Error>),
    Edit(EditingMessage),
}

#[doc(hidden)]
pub struct PdmDashboardStatusRow {
    _interval: Interval,
    loading: bool,
    edit: bool,

    async_pool: AsyncPool,
    subscription_info: LoadResult<PdmSubscriptionInfo, Error>,
}

impl PdmDashboardStatusRow {
    fn create_interval(ctx: &yew::Context<Self>) -> Interval {
        let link = ctx.link().clone();
        let _interval = Interval::new(
            ctx.props().reload_interval_s.saturating_mul(1000),
            move || {
                link.send_message(Msg::Reload(false));
            },
        );

        _interval
    }

    fn load_subscription(&self, ctx: &yew::Context<Self>) {
        // only load the subscription info here in custom views
        if ctx.props().editing_state.is_none() {
            return;
        }
        self.async_pool.send_future(ctx.link().clone(), async move {
            let res = proxmox_yew_comp::http_get("/nodes/localhost/subscription", None).await;
            Msg::SubscriptionInfoLoaded(res)
        });
    }
}

impl Component for PdmDashboardStatusRow {
    type Message = Msg;
    type Properties = DashboardStatusRow;

    fn create(ctx: &yew::Context<Self>) -> Self {
        let this = Self {
            _interval: Self::create_interval(ctx),
            loading: false,
            edit: false,
            async_pool: AsyncPool::new(),
            subscription_info: LoadResult::new(),
        };
        this.load_subscription(ctx);
        this
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let props = ctx.props();
        match msg {
            Msg::Reload(clicked) => {
                props.on_reload.emit(clicked);
                self.load_subscription(ctx);
                self.loading = true;
                true
            }
            Msg::Edit(editing) => {
                self.edit = matches!(editing, EditingMessage::Start);
                if let Some(state) = props.editing_state.as_ref() {
                    state.write().push(editing);
                }
                true
            }
            Msg::SubscriptionInfoLoaded(res) => {
                self.subscription_info.update(res);
                true
            }
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        self._interval = Self::create_interval(ctx);
        let new_refresh = ctx.props().last_refresh;
        if new_refresh.is_some() && old_props.last_refresh != new_refresh {
            self.loading = false;
        }
        true
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();
        let is_custom_view = props.editing_state.is_some();
        let is_loading = props.last_refresh.is_none()
            || self.loading
            || (is_custom_view && !self.subscription_info.has_data());
        let on_settings_click = props.on_settings_click.clone();
        Row::new()
            .gap(1)
            .class(AlignItems::Center)
            .with_child(
                Tooltip::new(
                    ActionIcon::new(if is_loading {
                        "fa fa-refresh fa-spin"
                    } else {
                        "fa fa-refresh"
                    })
                    .tabindex(0)
                    .disabled(is_loading)
                    .on_activate(ctx.link().callback(|_| Msg::Reload(true))),
                )
                .tip(tr!("Refresh now")),
            )
            .with_child(Container::new().with_child(match ctx.props().last_refresh {
                Some(last_refresh) => {
                    let date = render_epoch(last_refresh as i64);
                    tr!("Last refresh: {0}", date)
                }
                None => tr!("Now refreshing"),
            }))
            .with_flex_spacer()
            .with_optional_child(if is_custom_view {
                create_subscription_notice(&self.subscription_info).map(|element| {
                    element.class("pwt-pointer").onclick({
                        let link = ctx.link().clone();
                        move |_| {
                            if let Some(nav) = link.navigator() {
                                nav.push(&AnyRoute::new("/subscription"));
                            }
                        }
                    })
                })
            } else {
                None
            })
            .with_flex_spacer()
            .with_optional_child(props.editing_state.clone().and_then(|_| {
                (!self.edit).then_some({
                    Tooltip::new(ActionIcon::new("fa fa-pencil").tabindex(0).on_activate({
                        ctx.link()
                            .callback(move |_| Msg::Edit(EditingMessage::Start))
                    }))
                    .tip(tr!("Edit"))
                })
            }))
            .with_optional_child(props.editing_state.clone().and_then(|_| {
                self.edit.then_some({
                    Tooltip::new(
                        ActionIcon::new("fa fa-check")
                            .class(css::ColorScheme::Success)
                            .tabindex(0)
                            .on_activate({
                                ctx.link()
                                    .callback(move |_| Msg::Edit(EditingMessage::Finish))
                            }),
                    )
                    .tip(tr!("Finish Editing"))
                })
            }))
            .with_optional_child(props.editing_state.clone().and_then(|_| {
                self.edit.then_some({
                    Tooltip::new(
                        ActionIcon::new("fa fa-times")
                            .class(css::ColorScheme::Error)
                            .tabindex(0)
                            .on_activate({
                                ctx.link()
                                    .callback(move |_| Msg::Edit(EditingMessage::Cancel))
                            }),
                    )
                    .tip(tr!("Cancel Editing"))
                })
            }))
            .with_child(
                Tooltip::new(
                    ActionIcon::new("fa fa-cogs")
                        .tabindex(0)
                        .on_activate(move |_| on_settings_click.emit(())),
                )
                .tip(tr!("Dashboard Settings")),
            )
            .into()
    }
}

fn create_subscription_notice(
    subscriptions: &LoadResult<PdmSubscriptionInfo, Error>,
) -> Option<Tooltip> {
    if !subscriptions.has_data() {
        return None;
    }
    let mut tooltip = None;
    if let Some(subscriptions) = &subscriptions.data {
        if subscriptions.statistics.total_nodes == 0 {
            return None;
        } else if let SubscriptionStatus::Active = subscriptions.info.status {
            return None;
        }
    } else if let Some(err) = &subscriptions.error {
        tooltip = Some(err.to_string())
    }
    // only get here if there are remotes and PDM-subscription failed.
    let text = tr!(
        "Too many remote nodes without basic or higher subscriptions! No access to Enterprise-Repository or Enterprise Support."
    );
    let icon = subscription_icon(&SubscriptionStatus::NotFound.to_string());

    Some(
        Tooltip::new(
            Row::new()
                .padding_x(2)
                .gap(2)
                .class(css::AlignItems::Center)
                .with_child(icon.large())
                .with_child(Container::new().with_child(text)),
        )
        .tip(tooltip),
    )
}
