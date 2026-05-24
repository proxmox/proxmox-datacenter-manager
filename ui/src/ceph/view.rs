//! Top-level Ceph view: a master/detail split with the cluster list on the left and the selected
//! cluster's detail panel on the right.
//!
//! Mirrors the PVE remote view (`crate::pve`): both panes are always visible, selecting a cluster
//! in the list updates the right pane. Selection is component-local state, no router route.

use std::rc::Rc;

use yew::virtual_dom::{VComp, VNode};

use pwt::css::{AlignItems, FlexFit, JustifyContent};
use pwt::prelude::*;
use pwt::state::NavigationContainer;
use pwt::widget::{Column, Container, Fa, Panel, Row};

use pdm_api_types::ceph::CephClusterListEntry;

use super::cluster::CephClusterPanel;
use super::clusters::CephClusterListPanel;

#[derive(PartialEq, Properties)]
pub struct CephView;

impl CephView {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

impl From<CephView> for VNode {
    fn from(val: CephView) -> Self {
        VComp::new::<PdmCephView>(Rc::new(val), None).into()
    }
}

pub enum Msg {
    Select(CephClusterListEntry),
}

pub struct PdmCephView {
    selected: Option<CephClusterListEntry>,
}

impl yew::Component for PdmCephView {
    type Message = Msg;
    type Properties = CephView;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self { selected: None }
    }

    fn update(&mut self, _ctx: &yew::Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Select(entry) => self.selected = Some(entry),
        }
        true
    }

    fn view(&self, ctx: &yew::Context<Self>) -> Html {
        let link = ctx.link();

        let list_title: Html = Row::new()
            .gap(2)
            .class(AlignItems::Center)
            .with_child(Fa::new("ceph"))
            .with_child(tr!("Ceph Clusters"))
            .into();

        let list = Panel::new()
            .min_width(360)
            // The list carries the at-a-glance overview (health, remote, capacity, usage, OSD/mon
            // counts), so give it the larger share.
            .style("flex", "4 1 0")
            .class(FlexFit)
            .border(true)
            .title(list_title)
            .with_child(CephClusterListPanel::new().on_select(link.callback(Msg::Select)));

        let detail_content: Html = match &self.selected {
            Some(entry) => CephClusterPanel::new(entry.cluster.clone(), entry.display_name.clone())
                .remote(entry.remote.clone())
                .node(entry.node.clone())
                .into(),
            None => placeholder(),
        };

        let detail = Panel::new()
            .min_width(400)
            // Narrower share than the list; the tab bar scrolls and the dense tables (OSDs, pools)
            // scroll horizontally if needed.
            .style("flex", "3 1 0")
            .class(FlexFit)
            .border(true)
            .with_child(NavigationContainer::new().with_child(detail_content));

        let content = Container::new()
            .class("pwt-content-spacer")
            .class(FlexFit)
            .class("pwt-flex-direction-row")
            .with_child(list)
            .with_child(detail);

        NavigationContainer::new()
            .with_child(Panel::new().class(FlexFit).with_child(content))
            .into()
    }
}

/// Empty-state hint for the detail pane when no cluster is selected.
fn placeholder() -> Html {
    Column::new()
        .class(FlexFit)
        .class(AlignItems::Center)
        .class(JustifyContent::Center)
        .gap(2)
        .with_child(Fa::new("ceph").large_4x())
        .with_child(html! { { tr!("Select a Ceph cluster to view its details.") } })
        .into()
}
