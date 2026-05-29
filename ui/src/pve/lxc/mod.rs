mod overview;
use overview::LxcOverviewPanel;
use proxmox_yew_comp::configuration::pve::{
    LxcDnsPanel, LxcNetworkPanel, LxcOptionsPanel, LxcResourcesPanel,
};

use std::rc::Rc;

use yew::virtual_dom::{VComp, VNode};

use proxmox_deb_version::Version;
use pwt::css::FlexFit;
use pwt::prelude::*;
use pwt::widget::{Button, Column, Container, Fa, Row, TabBarItem, TabPanel, Tooltip};
use pwt_macros::builder;

use pdm_api_types::resource::PveLxcResource;

use crate::pve::utils::render_lxc_name;
use crate::pve::{GuestInfo, GuestType};
use crate::renderer::render_title_row;
use crate::widget::SnapshotWindow;

#[derive(Clone, Debug, Properties, PartialEq)]
#[builder]
pub struct LxcPanel {
    remote: String,
    node: String,
    info: PveLxcResource,

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

impl LxcPanel {
    pub fn new(remote: String, node: String, info: PveLxcResource) -> Self {
        yew::props!(Self { remote, node, info })
    }
}

pub struct LxcPanelComp {}

impl yew::Component for LxcPanelComp {
    type Message = ();
    type Properties = LxcPanel;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self {}
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();

        let title: Html = Row::new()
            .gap(2)
            .class(pwt::css::AlignItems::Baseline)
            .with_child(Fa::new("cube"))
            .with_child(tr! {"CT {0}", render_lxc_name(&props.info, true)})
            .into();

        TabPanel::new()
            .router(true)
            .class(pwt::css::FlexFit)
            .title(title)
            .tool(
                Tooltip::new(
                    Button::new(tr!("Open Web UI"))
                        .icon_class("fa fa-external-link")
                        .aria_label(tr!("Open the web UI of container {0}.", props.info.vmid))
                        .on_activate({
                            let link = ctx.link().clone();
                            let remote = props.remote.clone();
                            let node = props.node.clone();
                            let vmid = props.info.vmid;
                            move |_| {
                                let id = format!("lxc/{vmid}");
                                if let Some(url) =
                                    crate::get_deep_url(&link, &remote, Some(&node), &id)
                                {
                                    let _ = web_sys::window().unwrap().open_with_url(&url.href());
                                }
                            }
                        }),
                )
                .tip(tr!("Open the web UI of container {0}.", props.info.vmid)),
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
                        LxcOverviewPanel::new(remote.clone(), node.clone(), info.clone()).into()
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
                                    .with_child(render_title_row(tr!("Resources"), "cube"))
                                    .with_child(html! {<hr/>})
                                    .with_child(
                                        LxcResourcesPanel::new(node.clone(), vmid)
                                            .pve_manager_version(pve_manager_version.clone())
                                            .readonly(true)
                                            .remote(remote.clone()),
                                    )
                                    .with_child(
                                        render_title_row(tr!("Network"), "exchange").margin_top(6),
                                    )
                                    .with_child(html! {<hr/>})
                                    .with_child(
                                        LxcNetworkPanel::new(node.clone(), vmid)
                                            .pve_manager_version(pve_manager_version.clone())
                                            .readonly(true)
                                            .remote(remote.clone()),
                                    )
                                    .with_child(render_title_row(tr!("DNS"), "globe").margin_top(6))
                                    .with_child(html! {<hr/>})
                                    .with_child(
                                        LxcDnsPanel::new(node.clone(), vmid)
                                            .pve_manager_version(pve_manager_version.clone())
                                            .readonly(true)
                                            .remote(remote.clone()),
                                    )
                                    .with_child(
                                        render_title_row(tr!("Options"), "gear").margin_top(6),
                                    )
                                    .with_child(html! {<hr/>})
                                    .with_child(
                                        LxcOptionsPanel::new(node.clone(), vmid)
                                            .pve_manager_version(pve_manager_version.clone())
                                            .readonly(true)
                                            .remote(remote.clone()),
                                    ),
                            )
                            .into()
                    }
                },
            )
            .with_item_builder(
                TabBarItem::new()
                    .key("snapshots")
                    .label(tr!("Snapshots"))
                    .icon_class("fa fa-history"),
                {
                    let remote = props.remote.clone();
                    let vmid = props.info.vmid;
                    move |_| {
                        SnapshotWindow::new(
                            remote.clone(),
                            GuestInfo {
                                guest_type: GuestType::Lxc,
                                vmid,
                            },
                        )
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
                    let supported = props
                        .pve_manager_version
                        .as_ref()
                        .map(|ver| ver >= &Version::new("9.1.0", None))
                        .unwrap_or(true);
                    let vmid = props.info.vmid;
                    move |_| {
                        if supported {
                            let mut xtermjs = proxmox_yew_comp::XTermJs::new();
                            xtermjs.set_node_name(node.clone());
                            xtermjs.set_console_type(proxmox_yew_comp::ConsoleType::RemotePveLXC(
                                remote.clone(),
                                vmid as u64,
                            ));
                            xtermjs.into()
                        } else {
                            Row::new()
                                .class(pwt::css::FlexFit)
                                .class(pwt::css::JustifyContent::Center)
                                .class(pwt::css::AlignItems::Center)
                                .with_child(html! { tr!("pve-manager version too old") })
                                .into()
                        }
                    }
                },
            )
            .into()
    }
}

impl From<LxcPanel> for VNode {
    fn from(val: LxcPanel) -> Self {
        VComp::new::<LxcPanelComp>(Rc::new(val), None).into()
    }
}
