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

pub(crate) mod overview;

use overview::PbsNodeOverviewPanel;

use crate::get_deep_url_low_level;

#[derive(Clone, Debug, Eq, PartialEq, Properties)]
pub struct PbsNodePanel {
    /// The remote to show
    pub remote: String,
}

impl PbsNodePanel {
    pub fn new(remote: String) -> Self {
        yew::props!(Self { remote })
    }
}

impl From<PbsNodePanel> for VNode {
    fn from(val: PbsNodePanel) -> Self {
        VComp::new::<PbsNodePanelComp>(Rc::new(val), None).into()
    }
}

struct PbsNodePanelComp;

impl yew::Component for PbsNodePanelComp {
    type Message = ();
    type Properties = PbsNodePanel;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        props.remote != old_props.remote
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();

        let title: Html = Row::new()
            .gap(2)
            .class(AlignItems::Baseline)
            .with_child(Fa::new("building"))
            .with_child(tr!("Node"))
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
                    move |_| PbsNodeOverviewPanel::new(remote.clone()).into()
                },
            )
            .with_item_builder(
                TabBarItem::new()
                    .key("update_view")
                    .label(tr!("Updates"))
                    .icon_class("fa fa-refresh"),
                {
                    let remote = props.remote.clone();
                    let link = ctx.link().clone();
                    move |_| {
                        let base_url = format!("/pbs/remotes/{remote}/nodes/localhost/apt");
                        let task_base_url = format!("/pbs/remotes/{remote}/tasks");

                        AptPackageManager::new()
                            .base_url(base_url)
                            .task_base_url(task_base_url)
                            .enable_upgrade(true)
                            .on_upgrade({
                                let remote = remote.clone();
                                let link = link.clone();

                                move |_| {
                                    let hash = "#pbsServerAdministration:updates";
                                    if let Some(url) =
                                        get_deep_url_low_level(&link, &remote, None, hash)
                                    {
                                        let _ = gloo_utils::window().open_with_url(&url.href());
                                    }
                                }
                            })
                            .into()
                    }
                },
            )
            .into()
    }
}
