use std::rc::Rc;

use anyhow::Error;
use yew::{
    virtual_dom::{VComp, VNode},
    Component, Html, Properties,
};

use proxmox_yew_comp::Status;
use pwt::prelude::*;
use pwt::widget::{Button, Column, Container, Dialog, Fa, Panel, Row};
use pwt::{
    css::{AlignItems, FlexFit, JustifyContent, TextAlign},
    state::SharedState,
};

use pdm_api_types::subscription::{RemoteSubscriptionState, RemoteSubscriptions};

use crate::{dashboard::SubscriptionsList, LoadResult};

#[derive(Properties, PartialEq)]
pub struct SubscriptionInfo {
    subs: Option<Vec<RemoteSubscriptions>>,
}

impl SubscriptionInfo {
    pub fn new(subs: Option<Vec<RemoteSubscriptions>>) -> Self {
        Self { subs }
    }
}

struct PdmSubscriptionInfo;

fn render_subscription_status(subs: &[RemoteSubscriptions]) -> Row {
    let mut none = 0;
    let mut mixed = 0;
    let mut active = 0;
    let mut unknown = 0;
    for sub in subs {
        if sub.error.is_some() {
            unknown += 1;
            continue;
        }
        match sub.state {
            RemoteSubscriptionState::None => none += 1,
            RemoteSubscriptionState::Mixed => mixed += 1,
            RemoteSubscriptionState::Active => active += 1,
            RemoteSubscriptionState::Unknown => unknown += 1,
        }
    }

    let (status, title, msg) = if none > 0 {
        let msg = tr!(
            "At least one remote does not have a valid subscription. Please visit <a target=\"_blank\" href=\"https://www.proxmox.com\">www.proxmox.com</a> to get a list of available options."
        );

        let msg = Html::from_html_unchecked(msg.into());
        (Status::Error, tr!("No valid subscription"), msg)
    } else if mixed > 0 {
        (Status::Warning, tr!("Mixed Subscriptions"), tr!("At least one remote has mixed levels of subscription. These remotes fall back to the lowest one.").into())
    } else if unknown > 0 {
        (
            Status::Unknown,
            tr!("Unknown subscriptions"),
            tr!("At least one remote could not be queried and has an unknown subscription status.")
                .into(),
        )
    } else if active > 0 {
        (
            Status::Success,
            tr!("Your subscription status is valid."),
            "".into(),
        )
    } else {
        (
            Status::Unknown,
            tr!("Unknown subscriptions"),
            tr!("No remotes configured or could not fetch status.").into(),
        )
    };

    Row::new()
        .class(AlignItems::Center)
        .class(FlexFit)
        .padding(4)
        .with_child(Fa::from(status).large_4x().padding(4))
        .with_child(
            Column::new()
                .class(FlexFit)
                .padding(2)
                .class(AlignItems::Center)
                .class(TextAlign::Center)
                .with_child(html! {<h1 class="pwt-font-headline-medium">{title}</h1>})
                .with_child(Container::new().with_child(msg)),
        )
}

impl Component for PdmSubscriptionInfo {
    type Message = ();
    type Properties = SubscriptionInfo;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self {}
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();
        Column::new()
            .class(FlexFit)
            .class(JustifyContent::Center)
            .class(AlignItems::Center)
            .with_optional_child(
                props.subs.is_none().then_some(
                    Container::new()
                        .padding(4)
                        .with_child(Container::from_tag("i").class("pwt-loading-icon")),
                ),
            )
            .with_optional_child(
                props
                    .subs
                    .as_ref()
                    .map(|subs| render_subscription_status(subs)),
            )
            .into()
    }
}

impl From<SubscriptionInfo> for VNode {
    fn from(val: SubscriptionInfo) -> Self {
        let comp = VComp::new::<PdmSubscriptionInfo>(Rc::new(val), None);
        VNode::from(comp)
    }
}

pub fn create_subscriptions_dialog(
    subs: SharedState<LoadResult<Vec<RemoteSubscriptions>, Error>>,
    on_dialog_close: Callback<()>,
    on_refresh: Callback<MouseEvent>,
) -> Dialog {
    let loading = !subs.read().has_data();
    let subs = subs.read().data.clone();
    let subs = subs.unwrap_or_default();
    Dialog::new(tr!("Your Subscriptions"))
        .with_tool(Button::refresh(loading).on_activate(on_refresh))
        .resizable(true)
        .width(600)
        .height(600)
        .min_width(200)
        .min_height(200)
        .with_child(SubscriptionsList::new(subs))
        .on_close(on_dialog_close)
}

pub fn create_subscription_panel(
    subs: SharedState<LoadResult<Vec<RemoteSubscriptions>, Error>>,
    on_details_clicked: Callback<MouseEvent>,
) -> Panel {
    let title: Html = Row::new()
        .class(AlignItems::Center)
        .gap(2)
        .with_child(Fa::new("ticket"))
        .with_child(tr!("Subscription Status"))
        .into();

    let mut panel = Panel::new()
        .title(title)
        .border(true)
        .with_child(SubscriptionInfo::new(subs.read().data.clone()));

    if subs.read().data.is_some() {
        panel.add_tool(Button::new(tr!("Details")).onclick(on_details_clicked));
    }

    panel
}
