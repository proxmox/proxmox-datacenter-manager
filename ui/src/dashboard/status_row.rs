use gloo_timers::callback::Interval;
use yew::html::IntoPropValue;
use yew::{Component, Properties};

use pwt::css;
use pwt::prelude::*;
use pwt::state::SharedState;
use pwt::{
    css::AlignItems,
    widget::{ActionIcon, Container, Row, Tooltip},
};
use pwt_macros::{builder, widget};

use proxmox_yew_comp::utils::render_epoch;

use crate::dashboard::view::EditingMessage;

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
    Edit(EditingMessage),
}

#[doc(hidden)]
pub struct PdmDashboardStatusRow {
    _interval: Interval,
    loading: bool,
    edit: bool,
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
}

impl Component for PdmDashboardStatusRow {
    type Message = Msg;
    type Properties = DashboardStatusRow;

    fn create(ctx: &yew::Context<Self>) -> Self {
        Self {
            _interval: Self::create_interval(ctx),
            loading: false,
            edit: false,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let props = ctx.props();
        match msg {
            Msg::Reload(clicked) => {
                props.on_reload.emit(clicked);
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
        let is_loading = props.last_refresh.is_none() || self.loading;
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
