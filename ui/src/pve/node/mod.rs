use std::rc::Rc;

use proxmox_yew_comp::AptPackageManager;
use yew::{
    virtual_dom::{VComp, VNode},
    Context,
};

use pwt::{
    css::{AlignItems, ColorScheme},
    prelude::*,
    props::{ContainerBuilder, WidgetBuilder},
    widget::{Fa, Row, TabBarItem, TabPanel},
};

mod overview;

use overview::NodeOverviewPanel;

#[derive(Clone, Debug, Eq, PartialEq, Properties)]
pub struct NodePanel {
    /// The remote to show
    pub remote: String,

    /// The node to show
    pub node: String,
}

impl NodePanel {
    pub fn new(remote: String, node: String) -> Self {
        yew::props!(Self { remote, node })
    }
}

impl Into<VNode> for NodePanel {
    fn into(self) -> VNode {
        VComp::new::<NodePanelComp>(Rc::new(self), None).into()
    }
}

pub struct NodePanelComp;

impl yew::Component for NodePanelComp {
    type Message = ();
    type Properties = NodePanel;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        if props.remote != old_props.remote || props.node != old_props.node {
            true
        } else {
            false
        }
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
                    move |_| NodeOverviewPanel::new(remote.clone(), node.clone()).into()
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
                    move |_| {
                        let base_url = format!("/pve/remotes/{remote}/nodes/{node}/apt");
                        let task_base_url = format!("/pve/remotes/{remote}/tasks");

                        AptPackageManager::new()
                            .base_url(base_url)
                            .task_base_url(task_base_url)
                            .enable_upgrade(false)
                            .into()
                    }
                },
            )
            .into()
    }
}
