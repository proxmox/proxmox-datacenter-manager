mod overview;
use overview::QemuOverviewPanel;

use std::rc::Rc;

use yew::virtual_dom::{VComp, VNode};

use proxmox_deb_version::Version;
use pwt::css::FlexFit;
use pwt::prelude::*;
use pwt::widget::{Button, Column, Container, Fa, Row, TabBarItem, TabPanel, Tooltip};
use pwt_macros::builder;

use proxmox_yew_comp::configuration::pve::{QemuHardwarePanel, QemuOptionsPanel};

use pdm_api_types::resource::PveQemuResource;

use crate::pve::utils::render_qemu_name;
use crate::renderer::render_title_row;

#[derive(Clone, Debug, Properties, PartialEq)]
#[builder]
pub struct QemuPanel {
    remote: String,
    node: String,
    info: PveQemuResource,

    #[prop_or_default]
    #[builder]
    /// The nodes pve-manager version, used to feature gate some entries.
    pve_manager_version: Option<Version>,

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
            .router(true)
            .class(pwt::css::FlexFit)
            .title(title)
            .tool(
                Tooltip::new(
                    Button::new(tr!("Open Web UI"))
                        .icon_class("fa fa-external-link")
                        .aria_label(tr!("Open the web UI of VM {0}.", props.info.vmid))
                        .on_activate({
                            let link = ctx.link().clone();
                            let remote = props.remote.clone();
                            let node = props.node.clone();
                            let vmid = props.info.vmid;
                            move |_| {
                                let id = format!("qemu/{vmid}");
                                if let Some(url) =
                                    crate::get_deep_url(&link, &remote, Some(&node), &id)
                                {
                                    let _ = web_sys::window().unwrap().open_with_url(&url.href());
                                }
                            }
                        }),
                )
                .tip(tr!("Open the web UI of VM {0}.", props.info.vmid)),
            )
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
                    .key("config")
                    .label(tr!("Config"))
                    .icon_class("fa fa-file-text-o"),
                {
                    let remote = props.remote.clone();
                    let node = props.node.clone();
                    let vmid = props.info.vmid;
                    let pve_manager_version = props.pve_manager_version.clone();
                    move |_| {
                        Container::new()
                            .class(FlexFit)
                            .with_child(
                                Column::new()
                                    .padding(4)
                                    .gap(2)
                                    .with_child(render_title_row(tr!("Hardware"), "desktop"))
                                    .with_child(html! {<hr/>})
                                    .with_child(
                                        QemuHardwarePanel::new(node.clone(), vmid)
                                            .readonly(true)
                                            .remote(remote.clone()),
                                    )
                                    .with_child(
                                        render_title_row(tr!("Options"), "gear").margin_top(6),
                                    )
                                    .with_child(html! {<hr/>})
                                    .with_child(
                                        QemuOptionsPanel::new(node.clone(), vmid)
                                            .pve_manager_version(pve_manager_version.clone())
                                            .readonly(true)
                                            .remote(remote.clone()),
                                    ),
                            )
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
