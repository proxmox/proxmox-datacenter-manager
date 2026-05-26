//! Ceph managers + metadata servers tab.
//!
//! Mirrors PVE's habit of stacking the smaller service-daemon grids together: the managers (`/mgr`)
//! and the metadata servers (`/mds`) share this tab as two grids, since both are lightweight
//! daemons reported the same way.

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;

use yew::virtual_dom::{Key, VComp, VNode};

use pwt::css::AlignItems;
use pwt::prelude::*;
use pwt::state::{Selection, Store};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::{Button, Column, Fa, Row, Toolbar};

use proxmox_yew_comp::{
    LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState, Status,
};

use pdm_client::types::{CephMds, CephMgr};

use crate::renderer::render_title_row;

/// tr!-mapped label for a ceph-mgr / ceph-mds daemon state token, keeping the exact text for any
/// state outside the known set so an unexpected value still shows.
fn daemon_state_label(state: &str) -> String {
    match state {
        "active" => tr!("active"),
        "standby" => tr!("standby"),
        "standby-replay" => tr!("standby-replay"),
        "up:active" => tr!("up:active"),
        "up:standby" => tr!("up:standby"),
        "up:standby-replay" => tr!("up:standby-replay"),
        "laggy" => tr!("laggy"),
        "damaged" => tr!("damaged"),
        other => other.to_string(),
    }
}

async fn load_mgrs(cluster: &str) -> Result<Vec<CephMgr>, Error> {
    proxmox_yew_comp::http_get(format!("/ceph/clusters/{cluster}/mgr"), None).await
}

async fn load_mds(cluster: &str) -> Result<Vec<CephMds>, Error> {
    proxmox_yew_comp::http_get(format!("/ceph/clusters/{cluster}/mds"), None).await
}

#[derive(PartialEq, Properties)]
pub struct CephManagersPanel {
    cluster: AttrValue,
}

impl CephManagersPanel {
    pub fn new(cluster: impl Into<AttrValue>) -> Self {
        yew::props!(Self {
            cluster: cluster.into()
        })
    }
}

pub struct PdmCephManagersPanel {
    state: LoadableComponentState<()>,
    mgr_store: Store<CephMgr>,
    mds_store: Store<CephMds>,
    // row-highlight only, no action on select
    mgr_selection: Selection,
    mds_selection: Selection,
    mgr_columns: Rc<Vec<DataTableHeader<CephMgr>>>,
    mds_columns: Rc<Vec<DataTableHeader<CephMds>>>,
}

pwt::impl_deref_mut_property!(PdmCephManagersPanel, state, LoadableComponentState<()>);

impl LoadableComponent for PdmCephManagersPanel {
    type Message = ();
    type Properties = CephManagersPanel;
    type ViewState = ();

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        ctx.link().repeated_load(15_000);
        Self {
            state: LoadableComponentState::new(),
            mgr_store: Store::with_extract_key(|m: &CephMgr| Key::from(m.name.clone())),
            mds_store: Store::with_extract_key(|m: &CephMds| Key::from(m.name.clone())),
            mgr_selection: Selection::new(),
            mds_selection: Selection::new(),
            mgr_columns: mgr_columns(),
            mds_columns: mds_columns(),
        }
    }

    fn load(
        &self,
        ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        let cluster = ctx.props().cluster.clone();
        let mgr_store = self.mgr_store.clone();
        let mds_store = self.mds_store.clone();
        Box::pin(async move {
            // Resolve both before taking any store borrow.
            let mgrs = load_mgrs(&cluster).await?;
            let mds = load_mds(&cluster).await?;
            mgr_store.write().set_data(mgrs);
            mds_store.write().set_data(mds);
            Ok(())
        })
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        let link = ctx.link().clone();
        let loading = self.loading();
        Some(
            Toolbar::new()
                .class("pwt-overflow-hidden")
                .class("pwt-border-bottom")
                .with_flex_spacer()
                .with_child(Button::refresh(loading).onclick(move |_| link.send_reload()))
                .into(),
        )
    }

    fn main_view(&self, _ctx: &LoadableComponentContext<Self>) -> Html {
        // No FlexFit: let each grid size to its content and stack at the top (the tab content area
        // scrolls if both lists are large), instead of each stretching to half the pane and leaving
        // dead space.
        Column::new()
            .padding(2)
            .gap(2)
            .with_child(render_title_row(tr!("Managers"), "cogs"))
            .with_child(
                DataTable::new(Rc::clone(&self.mgr_columns), self.mgr_store.clone())
                    .border(true)
                    .selection(self.mgr_selection.clone()),
            )
            .with_child(render_title_row(tr!("Metadata Servers"), "folder-open-o"))
            .with_child(
                DataTable::new(Rc::clone(&self.mds_columns), self.mds_store.clone())
                    .border(true)
                    .selection(self.mds_selection.clone()),
            )
            .into()
    }
}

impl From<CephManagersPanel> for VNode {
    fn from(val: CephManagersPanel) -> Self {
        VNode::from(VComp::new::<LoadableComponentMaster<PdmCephManagersPanel>>(
            Rc::new(val),
            None,
        ))
    }
}

/// A running manager is healthy whether it is `active` or `standby` (a standby is ready to take
/// over, not a problem); only an unexpected state is unknown.
fn mgr_columns() -> Rc<Vec<DataTableHeader<CephMgr>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Name"))
            .flex(1)
            .get_property(|m: &CephMgr| &m.name)
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Host"))
            .flex(1)
            .render(|m: &CephMgr| html! { { m.host.clone().unwrap_or_default() } })
            .into(),
        DataTableColumn::new(tr!("State"))
            .width("130px")
            .render(|m: &CephMgr| {
                let status = match m.state.as_str() {
                    "active" | "standby" => Status::Success,
                    _ => Status::Unknown,
                };
                Row::new()
                    .gap(2)
                    .class(AlignItems::Center)
                    .with_child(Fa::from(status))
                    .with_child(html! { { daemon_state_label(&m.state) } })
                    .into()
            })
            .into(),
        DataTableColumn::new(tr!("Address"))
            .flex(2)
            .render(|m: &CephMgr| html! { { m.addr.clone().unwrap_or_default() } })
            .into(),
        DataTableColumn::new(tr!("Version"))
            .width("120px")
            .render(|m: &CephMgr| html! { { m.ceph_version_short.clone().unwrap_or_default() } })
            .into(),
    ])
}

fn mds_columns() -> Rc<Vec<DataTableHeader<CephMds>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Name"))
            .flex(1)
            .get_property(|m: &CephMds| &m.name)
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Host"))
            .flex(1)
            .render(|m: &CephMds| html! { { m.host.clone().unwrap_or_default() } })
            .into(),
        DataTableColumn::new(tr!("Rank"))
            .width("70px")
            .justify("right")
            .render(|m: &CephMds| match m.rank {
                Some(r) if r >= 0 => html! { { r.to_string() } },
                _ => html! { {"-"} },
            })
            .into(),
        DataTableColumn::new(tr!("State"))
            .width("150px")
            .render(|m: &CephMds| {
                // up:active / up:standby[-replay] are healthy; standby is idle; anything else
                // (laggy, damaged) is noteworthy.
                let status = if m.state.contains("active") || m.state.contains("standby") {
                    Status::Success
                } else {
                    Status::Warning
                };
                Row::new()
                    .gap(2)
                    .class(AlignItems::Center)
                    .with_child(Fa::from(status))
                    .with_child(html! { { daemon_state_label(&m.state) } })
                    .into()
            })
            .into(),
        DataTableColumn::new(tr!("File System"))
            .flex(1)
            .render(|m: &CephMds| html! { { m.fs_name.clone().unwrap_or_default() } })
            .into(),
        DataTableColumn::new(tr!("Version"))
            .width("120px")
            .render(|m: &CephMds| html! { { m.ceph_version_short.clone().unwrap_or_default() } })
            .into(),
    ])
}
