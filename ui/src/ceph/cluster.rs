//! Per-cluster Ceph detail view: a tabbed panel over one cluster's resources.
//!
//! Rendered in the right-hand pane of the Ceph master/detail view (see
//! [`super::view`]). Phase 1b ships the read-only "Dashboard" tab; the
//! monitor/pool/flag/OSD-tree tabs plug into the same shell as they land.

use std::rc::Rc;

use gloo_utils::window;

use yew::virtual_dom::{Key, VComp, VNode};

use pwt::css::{AlignItems, ColorScheme, FlexFit};
use pwt::prelude::*;
use pwt::widget::{Button, Fa, MiniScrollMode, Row, TabBarItem, TabPanel};

use super::dashboard::CephDashboardPanel;
use super::monitors::CephMonitorsPanel;
use super::osds::CephOsdsPanel;
use crate::get_deep_url;

#[derive(PartialEq, Properties)]
pub struct CephClusterPanel {
    pub cluster: AttrValue,
    pub display_name: AttrValue,
    /// A representative PVE remote, for the "Open Web UI" escalation link.
    #[prop_or_default]
    pub remote: Option<String>,
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
}

impl From<CephClusterPanel> for VNode {
    fn from(val: CephClusterPanel) -> Self {
        // Key by fsid so selecting a different cluster remounts the panel (and
        // its dashboard), triggering a fresh load instead of reusing stale data.
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

        // Escalation link into the cluster's native PVE web UI (Ceph is managed
        // there for ops PDM does not cover). Only shown when a PVE remote backs
        // the cluster; a standalone-only cluster has no PVE UI to open.
        let open_web_ui = props.remote.clone().map(|remote| {
            let link = ctx.link().clone();
            Button::new(tr!("Open Web UI"))
                .icon_class("fa fa-external-link")
                .onclick(move |_| {
                    if let Some(url) = get_deep_url(&link, &remote, None, "") {
                        let _ = window().open_with_url(&url.href());
                    }
                })
        });

        let mut panel = TabPanel::new()
            .router(true)
            // Scroll the tab strip when it exceeds the (often half-width) detail
            // pane, so the tabs + tool never widen the panel and shift content.
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
                    .key("osds")
                    .label(tr!("OSDs"))
                    .icon_class("fa fa-hdd-o"),
                {
                    let cluster = props.cluster.clone();
                    move |_| CephOsdsPanel::new(cluster.clone()).into()
                },
            )
            .into()
    }
}
