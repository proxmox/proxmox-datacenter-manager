use gloo_timers::callback::Interval;
use js_sys::Date;
use yew::{Component, Properties};

use pwt::prelude::*;
use pwt::{
    css::AlignItems,
    widget::{ActionIcon, Container, Row, Tooltip},
};
use pwt_macros::widget;

use proxmox_yew_comp::utils::format_duration_human;

#[widget(comp=PdmDashboardStatusRow)]
#[derive(Properties, PartialEq, Clone)]
pub struct DashboardStatusRow {
    last_refresh: Option<f64>,
    reload_interval_s: u64,

    on_reload: Callback<bool>,

    on_settings_click: Callback<()>,
}

impl DashboardStatusRow {
    pub fn new(
        last_refresh: Option<f64>,
        reload_interval_s: u64,
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
    CheckReload,
}

#[doc(hidden)]
pub struct PdmDashboardStatusRow {
    _interval: Option<Interval>,
}

impl PdmDashboardStatusRow {
    fn update_interval(&mut self, ctx: &yew::Context<Self>) {
        let link = ctx.link().clone();
        let _interval = ctx.props().last_refresh.map(|_| {
            Interval::new(1000, move || {
                link.send_message(Msg::CheckReload);
            })
        });

        self._interval = _interval;
    }
}

impl Component for PdmDashboardStatusRow {
    type Message = Msg;
    type Properties = DashboardStatusRow;

    fn create(ctx: &yew::Context<Self>) -> Self {
        let mut this = Self { _interval: None };
        this.update_interval(ctx);
        this
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let props = ctx.props();
        match msg {
            Msg::Reload(clicked) => {
                props.on_reload.emit(clicked);
                true
            }
            Msg::CheckReload => match ctx.props().last_refresh {
                Some(last_refresh) => {
                    let duration = Date::now() / 1000.0 - last_refresh;
                    if duration >= props.reload_interval_s as f64 {
                        ctx.link().send_message(Msg::Reload(false));
                    }
                    true
                }
                None => false,
            },
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, _old_props: &Self::Properties) -> bool {
        self.update_interval(ctx);
        true
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();
        let is_loading = props.last_refresh.is_none();
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
                    let duration = Date::now() / 1000.0 - last_refresh;
                    tr!("Last refreshed: {0} ago", format_duration_human(duration))
                }
                None => tr!("Now refreshing"),
            }))
            .with_flex_spacer()
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
