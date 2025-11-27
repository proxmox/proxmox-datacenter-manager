mod overview;
use overview::QemuOverviewPanel;

use std::rc::Rc;

use yew::virtual_dom::{VComp, VNode};

use pwt::prelude::*;
use pwt::widget::{Fa, Row, TabBarItem, TabPanel};

use proxmox_yew_comp::configuration::pve::{QemuHardwarePanel, QemuOptionsPanel};

use pdm_api_types::resource::PveQemuResource;

use crate::pve::utils::render_qemu_name;

#[derive(Clone, Debug, Properties, PartialEq)]
pub struct QemuPanel {
    remote: String,
    node: String,
    info: PveQemuResource,

    #[prop_or(60_000)]
    /// The interval for refreshing the rrd data
    pub rrd_interval: u32,

    #[prop_or(10_000)]
    /// The interval for refreshing the status data
    pub status_interval: u32,
}

impl QemuPanel {
    pub fn new(remote: String, node: String, info: PveQemuResource) -> Self {
        yew::props!(Self { remote, node, info })
    }
}

pub struct QemuPanelComp {}

impl yew::Component for QemuPanelComp {
    type Message = ();
    type Properties = QemuPanel;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self {}
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();

        let title: Html = Row::new()
            .gap(2)
            .class(pwt::css::AlignItems::Baseline)
            .with_child(Fa::new("desktop"))
            .with_child(tr! {"VM '{0}'", render_qemu_name(&props.info, true)})
            .into();

        TabPanel::new()
            .class(pwt::css::FlexFit)
            .title(title)
            .with_item_builder(
                TabBarItem::new()
                    .key("status_view")
                    .label(tr!("Overview"))
                    .icon_class("fa fa-tachometer"),
                {
                    let remote = props.remote.clone();
                    let node = props.node.clone();
                    let info = props.info.clone();
                    move |_| {
                        QemuOverviewPanel::new(remote.clone(), node.clone(), info.clone()).into()
                    }
                },
            )
            .with_item_builder(
                TabBarItem::new()
                    .key("hardware")
                    .label(tr!("Hardware"))
                    .icon_class("fa fa-desktop"),
                {
                    let remote = props.remote.clone();
                    let node = props.node.clone();
                    let vmid = props.info.vmid;
                    move |_| {
                        QemuHardwarePanel::new(node.clone(), vmid)
                            .readonly(true)
                            .remote(remote.clone())
                            .into()
                    }
                },
            )
            .with_item_builder(
                TabBarItem::new()
                    .key("options")
                    .label(tr!("Options"))
                    .icon_class("fa fa-gear"),
                {
                    let remote = props.remote.clone();
                    let node = props.node.clone();
                    let vmid = props.info.vmid;
                    move |_| {
                        QemuOptionsPanel::new(node.clone(), vmid)
                            .readonly(true)
                            .remote(remote.clone())
                            .into()
                    }
                },
            )
            .into()
    }
}

impl From<QemuPanel> for VNode {
    fn from(val: QemuPanel) -> Self {
        VComp::new::<QemuPanelComp>(Rc::new(val), None).into()
    }
}
