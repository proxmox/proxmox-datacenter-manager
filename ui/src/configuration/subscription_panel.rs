use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use pdm_api_types::subscription::SubscriptionStatistics;
use serde_json::Value;

use yew::virtual_dom::{VComp, VNode};

use pwt::css;
use pwt::prelude::*;
use pwt::widget::{error_message, Button, Column, Container, Progress, Row, Toolbar};

use proxmox_yew_comp::{http_get, http_post, KVGrid, KVGridRow};
use proxmox_yew_comp::{
    LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState,
};

const SUBSCRIPTION_URL: &str = "/nodes/localhost/subscription";

#[derive(Properties, PartialEq, Clone)]
pub struct SubscriptionPanel {}

impl SubscriptionPanel {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

pub enum Msg {
    Checking,
    LoadFinished(Value),
}

pub struct ProxmoxSubscriptionPanel {
    state: LoadableComponentState<()>,
    rows: Rc<Vec<KVGridRow>>,
    data: Option<Rc<Value>>,
    loaded: bool,
    checking: bool,
}

pwt::impl_deref_mut_property!(
    ProxmoxSubscriptionPanel,
    state,
    LoadableComponentState<()>
);

impl LoadableComponent for ProxmoxSubscriptionPanel {
    type Message = Msg;
    type Properties = SubscriptionPanel;
    type ViewState = ();

    fn create(_ctx: &LoadableComponentContext<Self>) -> Self {
        Self {
            state: LoadableComponentState::new(),
            rows: Rc::new(rows()),
            data: None,
            loaded: false,
            checking: false,
        }
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadFinished(value) => {
                self.data = Some(Rc::new(value));
                self.loaded = true;
                self.checking = false;
            }
            Msg::Checking => {
                self.checking = true;
                let link = ctx.link();
                link.spawn({
                    let link = link.clone();
                    async move {
                        match http_post(SUBSCRIPTION_URL, None).await {
                            Ok(()) => link.send_reload(),
                            Err(err) => link.show_error(tr!("Error"), err.to_string(), true),
                        }
                    }
                });
            }
        }
        true
    }

    fn load(
        &self,
        _ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>>>> {
        let link = _ctx.link().clone();
        Box::pin(async move {
            let info = http_get(SUBSCRIPTION_URL, None).await?;
            link.send_message(Msg::LoadFinished(info));
            Ok(())
        })
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        let link = ctx.link().clone();
        let toolbar = Toolbar::new()
            .class("pwt-overflow-hidden")
            .border_bottom(true)
            .with_child(
                Button::new(tr!("Check"))
                    .disabled(self.checking)
                    .icon_class("fa fa-check-square-o")
                    .on_activate(link.callback(|_| Msg::Checking)),
            )
            .with_spacer()
            .with_flex_spacer()
            .with_child({
                let loading = self.loading();
                Button::refresh(loading || self.checking).on_activate(move |_| link.send_reload())
            });

        Some(toolbar.into())
    }

    fn main_view(&self, _ctx: &LoadableComponentContext<Self>) -> Html {
        let data = match &self.data {
            Some(data) => data.clone(),
            None => Rc::new(Value::Null),
        };

        Column::new()
            .class(css::FlexFit)
            .with_optional_child((!self.loaded).then_some(Progress::new()))
            .with_child(
                KVGrid::new()
                    .class(css::FlexFit)
                    .data(data.clone())
                    .rows(Rc::clone(&self.rows)),
            )
            .into()
    }
}

impl From<SubscriptionPanel> for VNode {
    fn from(val: SubscriptionPanel) -> Self {
        let comp =
            VComp::new::<LoadableComponentMaster<ProxmoxSubscriptionPanel>>(Rc::new(val), None);
        VNode::from(comp)
    }
}

// FIXME: ratios copied from backend, relay the result instead?

// minimum ratio of nodes with active basic or higher subscriptions
const SUBSCRIPTION_THRESHOLD: f64 = 0.8;

fn rows() -> Vec<KVGridRow> {
    vec![
        KVGridRow::new("status", tr!("Status")).renderer(move |_name, value, record| {
            let value = match value {
                Value::String(data) => data,
                Value::Null => return Container::from_tag("i").class("pwt-loading-icon").into(),
                _ => return error_message(&tr!("invalid data")).into(),
            };
            match record["message"].as_str() {
                Some(msg) => format!("{value}: {msg}").into(),
                None => value.into(),
            }
        }),
        KVGridRow::new("statistics", tr!("Statistics")).renderer(move |_name, value, _record| {
            let statistics = serde_json::from_value::<SubscriptionStatistics>(value.clone());
            match statistics {
                Ok(stats) => {
                    let basic_or_higher_ratio = if stats.total_nodes > 0 {
                        (stats.active_subscriptions - stats.community) as f64
                            / stats.total_nodes as f64
                    } else {
                        0.
                    };

                    let op = (basic_or_higher_ratio >= SUBSCRIPTION_THRESHOLD)
                        .then_some(">=")
                        .unwrap_or("<");

                    Column::new()
                        .with_child(Row::new().with_child(tr!(
                            "Subscription Ratio (Basic or Higher): {0} ({1} {2})",
                            format!("{:.0}%", basic_or_higher_ratio * 100.0),
                            op,
                            format!("{:.0}%", SUBSCRIPTION_THRESHOLD * 100.0),
                        )))
                        .into()
                }
                Err(err) => error_message(&format!("api error: {err}")).into(),
            }
        }),
        KVGridRow::new("url", tr!("Info URL")).renderer(|_name, value, _record| {
            let url = value.as_str().unwrap().to_string();
            html! { <a target="_blank" href={url.clone()}>{url}</a> }
        }),
    ]
}
