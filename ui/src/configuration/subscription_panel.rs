use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use pdm_api_types::subscription::SubscriptionStatistics;
use serde_json::Value;

use yew::virtual_dom::{VComp, VNode};

use pwt::prelude::*;
use pwt::widget::{error_message, Button, Column, Container, Row, Toolbar};

use proxmox_yew_comp::{http_get, http_post, KVGrid, KVGridRow};
use proxmox_yew_comp::{LoadableComponent, LoadableComponentContext, LoadableComponentMaster};

const SUBSCRIPTION_URL: &str = "/nodes/localhost/subscription";

#[derive(Properties, PartialEq, Clone)]
pub struct SubscriptionPanel {}

impl SubscriptionPanel {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

pub enum Msg {
    LoadFinished(Value),
}

pub struct ProxmoxSubscriptionPanel {
    rows: Rc<Vec<KVGridRow>>,
    data: Option<Rc<Value>>,
}

impl LoadableComponent for ProxmoxSubscriptionPanel {
    type Message = Msg;
    type Properties = SubscriptionPanel;
    type ViewState = ();

    fn create(_ctx: &LoadableComponentContext<Self>) -> Self {
        Self {
            rows: Rc::new(rows()),
            data: None,
        }
    }

    fn update(&mut self, _ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadFinished(value) => {
                self.data = Some(Rc::new(value));
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
        let toolbar = Toolbar::new()
            .class("pwt-overflow-hidden")
            .border_bottom(true)
            .with_child(
                Button::new(tr!("Check"))
                    .icon_class("fa fa-check-square-o")
                    .on_activate({
                        let link = ctx.link();

                        move |_| {
                            link.spawn({
                                let link = link.clone();
                                async move {
                                    match http_post(SUBSCRIPTION_URL, None).await {
                                        Ok(()) => link.send_reload(),
                                        Err(err) => {
                                            link.show_error(tr!("Error"), err.to_string(), true)
                                        }
                                    }
                                }
                            })
                        }
                    }),
            )
            .with_spacer()
            .with_flex_spacer()
            .with_child({
                let loading = ctx.loading();
                let link = ctx.link();
                Button::refresh(loading).on_activate(move |_| link.send_reload())
            });

        Some(toolbar.into())
    }

    fn main_view(&self, _ctx: &LoadableComponentContext<Self>) -> Html {
        let data = match &self.data {
            Some(data) => data.clone(),
            None => Rc::new(Value::Null),
        };

        KVGrid::new()
            .class("pwt-flex-fit")
            .data(data.clone())
            .rows(Rc::clone(&self.rows))
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

// FIXME: ratios copied from backend

// minimum ratio of nodes with active subscriptions
const SUBSCRIPTION_THRESHOLD: f64 = 0.9;
// max ratio of nodes with community subscriptions, among nodes with subscriptions
const COMMUNITY_THRESHOLD: f64 = 0.4;

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
                    let subscribed_ratio =
                        stats.active_subscriptions as f64 / stats.total_nodes as f64;
                    let community_ratio =
                        stats.community as f64 / stats.active_subscriptions as f64;

                    fn operator(a: f64, b: f64) -> &'static str {
                        if a >= b {
                            ">="
                        } else {
                            "<"
                        }
                    }

                    Column::new()
                        .with_child(Row::new().with_child(tr!(
                            "Subscribed Ratio: {0} ({1} {2})",
                            format!("{:.0}%", subscribed_ratio * 100.0),
                            operator(subscribed_ratio, SUBSCRIPTION_THRESHOLD),
                            format!("{:.0}%", SUBSCRIPTION_THRESHOLD * 100.0),
                        )))
                        .with_child(Row::new().with_child(tr!(
                            "Community Ratio: {0} ({1} {2})",
                            format!("{:.0}%", community_ratio * 100.0),
                            operator(community_ratio, COMMUNITY_THRESHOLD),
                            format!("{:.0}%", COMMUNITY_THRESHOLD * 100.0),
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
