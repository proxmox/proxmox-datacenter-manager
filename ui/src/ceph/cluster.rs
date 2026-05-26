//! Per-cluster Ceph detail view: a tabbed panel over one cluster's resources.
//!
//! Rendered in the right-hand pane of the Ceph master/detail view (see [`super::view`]). The first
//! tab is the dashboard; the monitor, manager, OSD, pool, CephFS and flags tabs share the same
//! shell.

use std::rc::Rc;

use gloo_utils::window;

use yew::virtual_dom::{Key, VComp, VNode};

use pwt::css::{AlignItems, ColorScheme, FlexFit};
use pwt::prelude::*;
use pwt::widget::{Button, Fa, MiniScrollMode, Row, TabBarItem, TabPanel};

use super::cephfs::CephFsPanel;
use super::dashboard::CephDashboardPanel;
use super::flags::CephFlagsPanel;
use super::managers::CephManagersPanel;
use super::monitors::CephMonitorsPanel;
use super::osds::CephOsdsPanel;
use super::pools::CephPoolsPanel;
use crate::{get_deep_url, get_deep_url_low_level};

#[derive(PartialEq, Properties)]
pub struct CephClusterPanel {
    pub cluster: AttrValue,
    pub display_name: AttrValue,
    /// A representative PVE remote, for the "Open Web UI" escalation link.
    #[prop_or_default]
    pub remote: Option<String>,
    /// A node on `remote` that backs the cluster, so the escalation link can open that node's Ceph
    /// panel directly rather than the remote root.
    #[prop_or_default]
    pub node: Option<String>,
}

impl CephClusterPanel {
    pub fn new(cluster: impl Into<AttrValue>, display_name: impl Into<AttrValue>) -> Self {
        yew::props!(Self {
            cluster: cluster.into(),
            display_name: display_name.into(),
        })
    }

    pub fn remote(mut self, remote: Option<String>) -> Self {
        self.remote = remote;
        self
    }

    pub fn node(mut self, node: Option<String>) -> Self {
        self.node = node;
        self
    }
}

impl From<CephClusterPanel> for VNode {
    fn from(val: CephClusterPanel) -> Self {
        // Key by fsid so selecting a different cluster remounts the panel (and its dashboard),
        // triggering a fresh load instead of reusing stale data.
        let key = Key::from(val.cluster.to_string());
        VComp::new::<PdmCephClusterPanel>(Rc::new(val), Some(key)).into()
    }
}

struct PdmCephClusterPanel;

impl yew::Component for PdmCephClusterPanel {
    type Message = ();
    type Properties = CephClusterPanel;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self
    }

    fn view(&self, ctx: &yew::Context<Self>) -> Html {
        let props = ctx.props();

        let title: Html = Row::new()
            .gap(2)
            .class(AlignItems::Center)
            .with_child(Fa::new("ceph"))
            .with_child(tr!("Ceph Cluster '{0}'", props.display_name))
            .into();

        // Escalation link into the cluster's native PVE web UI, where the Ceph ops PDM does not
        // cover are managed. Shown only when a PVE remote backs the cluster; a standalone-only
        // cluster has no PVE UI to open.
        let open_web_ui = props.remote.clone().map(|remote| {
            let link = ctx.link().clone();
            let node = props.node.clone();
            Button::new(tr!("Open Web UI"))
                .icon_class("fa fa-external-link")
                .onclick(move |_| {
                    // Mirror the active PDM Ceph subview onto the matching PVE Ceph tab. pwt's
                    // TabPanel.router(true) appends the active key as the trailing `/`-delimited
                    // segment of the URL hash; unknown values (e.g. before a sub-tab is selected)
                    // fall back to the top-level Ceph node panel.
                    let hash = window().location().hash().unwrap_or_default();
                    let pve_tab = match hash.rsplit('/').next().unwrap_or("") {
                        "monitors" => "40",
                        "osds" => "42",
                        "pools" => "43",
                        "cephfs" => "ceph-cephfspanel",
                        // PVE manages flags through a dialog opened from the OSD panel
                        "flags" => "42",
                        _ => "38",
                    };
                    let url = match &node {
                        Some(node) => get_deep_url_low_level(
                            &link,
                            &remote,
                            Some(node),
                            &format!("v1::=node/{node}::{pve_tab}"),
                        ),
                        None => get_deep_url(&link, &remote, None, ""),
                    };
                    if let Some(url) = url {
                        let _ = window().open_with_url(&url.href());
                    }
                })
        });

        let mut panel = TabPanel::new()
            .router(true)
            // Scroll the tab strip when it exceeds the (often half-width) detail pane, so the tabs
            // + tool never widen the panel and shift content.
            .scroll_mode(MiniScrollMode::Arrow)
            .class(FlexFit)
            .class(ColorScheme::Neutral)
            .title(title);
        if let Some(button) = open_web_ui {
            panel = panel.tool(button);
        }

        panel
            .with_item_builder(
                TabBarItem::new()
                    .key("dashboard")
                    .label(tr!("Dashboard"))
                    .icon_class("fa fa-tachometer"),
                {
                    let cluster = props.cluster.clone();
                    move |_| CephDashboardPanel::new(cluster.clone()).into()
                },
            )
            .with_item_builder(
                TabBarItem::new()
                    .key("monitors")
                    .label(tr!("Monitors"))
                    .icon_class("fa fa-television"),
                {
                    let cluster = props.cluster.clone();
                    move |_| CephMonitorsPanel::new(cluster.clone()).into()
                },
            )
            .with_item_builder(
                TabBarItem::new()
                    .key("managers")
                    .label(tr!("Managers"))
                    .icon_class("fa fa-cogs"),
                {
                    let cluster = props.cluster.clone();
                    move |_| CephManagersPanel::new(cluster.clone()).into()
                },
            )
            .with_item_builder(
                TabBarItem::new()
                    .key("osds")
                    .label(tr!("OSDs"))
                    .icon_class("fa fa-hdd-o"),
                {
                    let cluster = props.cluster.clone();
                    move |_| CephOsdsPanel::new(cluster.clone()).into()
                },
            )
            .with_item_builder(
                TabBarItem::new()
                    .key("pools")
                    .label(tr!("Pools"))
                    .icon_class("fa fa-database"),
                {
                    let cluster = props.cluster.clone();
                    move |_| CephPoolsPanel::new(cluster.clone()).into()
                },
            )
            .with_item_builder(
                TabBarItem::new()
                    .key("cephfs")
                    .label(tr!("CephFS"))
                    .icon_class("fa fa-folder-open-o"),
                {
                    let cluster = props.cluster.clone();
                    move |_| CephFsPanel::new(cluster.clone()).into()
                },
            )
            .with_item_builder(
                TabBarItem::new()
                    .key("flags")
                    .label(tr!("Flags"))
                    .icon_class("fa fa-flag"),
                {
                    let cluster = props.cluster.clone();
                    move |_| CephFlagsPanel::new(cluster.clone()).into()
                },
            )
            .into()
    }
}
