use std::rc::Rc;

use gloo_utils::window;
use proxmox_yew_comp::{AptPackageManager, ConsoleType, XTermJs};
use yew::virtual_dom::{VComp, VNode};

use pwt::{
    css::{AlignItems, ColorScheme},
    prelude::*,
    props::{ContainerBuilder, WidgetBuilder},
    widget::{Fa, Row, TabBarItem, TabPanel},
};

mod overview;

use overview::PveNodeOverviewPanel;

use crate::get_deep_url;

#[derive(Clone, Debug, Eq, PartialEq, Properties)]
pub struct PveNodePanel {
    /// The remote to show
    pub remote: String,

    /// The node to show
    pub node: String,
}

impl PveNodePanel {
    pub fn new(remote: String, node: String) -> Self {
        yew::props!(Self { remote, node })
    }
}

impl From<PveNodePanel> for VNode {
    fn from(val: PveNodePanel) -> Self {
        VComp::new::<PveNodePanelComp>(Rc::new(val), None).into()
    }
}

struct PveNodePanelComp;

impl yew::Component for PveNodePanelComp {
    type Message = ();
    type Properties = PveNodePanel;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();

        let title: Html = Row::new()
            .gap(2)
            .class(AlignItems::Baseline)
            .with_child(Fa::new("building"))
            .with_child(tr! {"Node '{0}'", props.node})
            .into();

        TabPanel::new()
            .router(true)
            .class(pwt::css::FlexFit)
            .title(title)
            .class(ColorScheme::Neutral)
            .with_item_builder(
                TabBarItem::new()
                    .key("status_view")
                    .label(tr!("Overview"))
                    .icon_class("fa fa-tachometer"),
                {
                    let remote = props.remote.clone();
                    let node = props.node.clone();
                    move |_| PveNodeOverviewPanel::new(remote.clone(), node.clone()).into()
                },
            )
            .with_item_builder(
                TabBarItem::new()
                    .key("update_view")
                    .label(tr!("Updates"))
                    .icon_class("fa fa-refresh"),
                {
                    let remote = props.remote.clone();
                    let node = props.node.clone();
                    let link = ctx.link().clone();
                    move |_| {
                        let base_url = format!("/pve/remotes/{remote}/nodes/{node}/apt");
                        let task_base_url = format!("/pve/remotes/{remote}/tasks");

                        AptPackageManager::new()
                            .base_url(base_url)
                            .task_base_url(task_base_url)
                            .enable_upgrade(true)
                            .on_upgrade({
                                let remote = remote.clone();
                                let link = link.clone();
                                let id = format!("node/{}::apt", node);
                                move |_| {
                                    if let Some(url) = get_deep_url(&link, &remote, None, &id) {
                                        let _ = window().open_with_url(&url.href());
                                    }
                                }
                            })
                            .into()
                    }
                },
            )
            .with_item_builder(
                TabBarItem::new()
                    .key("shell_view")
                    .label(tr!("Shell"))
                    .icon_class("fa fa-terminal"),
                {
                    let remote = props.remote.clone();
                    let node = props.node.clone();
                    move |_| {
                        let mut xtermjs = XTermJs::new();
                        xtermjs.set_node_name(node.clone());
                        xtermjs.set_console_type(ConsoleType::RemotePveLoginShell(remote.clone()));
                        xtermjs.into()
                    }
                },
            )
            .into()
    }
}
