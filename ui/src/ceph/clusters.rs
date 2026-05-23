//! `/ceph/clusters` - the datacenter-wide list of detected Ceph clusters.
//!
//! Read-only (phase 1b): each row shows the cluster's health, detection state
//! and member count. Drill-down into a single cluster's dashboard lands next.

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;

use yew::virtual_dom::{Key, VComp, VNode};

use proxmox_human_byte::HumanByte;

use pwt::css::{AlignItems, FontColor};
use pwt::prelude::*;
use pwt::state::{Selection, Store};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::{Button, Container, Fa, Row, Toolbar};

use proxmox_yew_comp::{
    LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState, Status,
};

use pdm_api_types::ceph::{CephClusterListEntry, CephClusterState};

use super::renderer::{
    ceph_cluster_state_label, ceph_health_label, ceph_health_severity, ceph_health_status,
    usage_cell,
};

/// Icon severity and label for the Health cell. A non-`Detected` cluster shows
/// its detection state (the health overlay is stale or unknown then), so the
/// list keeps surfacing unreachable/gone clusters without a separate column.
fn health_cell(item: &CephClusterListEntry) -> (Status, String) {
    match item.state {
        CephClusterState::Detected => (
            ceph_health_status(item.health.as_deref()),
            ceph_health_label(item.health.as_deref()),
        ),
        CephClusterState::Unreachable => (Status::Warning, ceph_cluster_state_label(item.state)),
        CephClusterState::Gone => (Status::Error, ceph_cluster_state_label(item.state)),
        CephClusterState::Tombstoned => (Status::Unknown, ceph_cluster_state_label(item.state)),
    }
}

/// Combined triage severity (higher is worse): a gone/unreachable cluster
/// outranks any health string, so it sorts to the top of the list.
fn row_severity(item: &CephClusterListEntry) -> u8 {
    match item.state {
        CephClusterState::Gone => 5,
        CephClusterState::Unreachable => 4,
        CephClusterState::Tombstoned => 1,
        CephClusterState::Detected => ceph_health_severity(item.health.as_deref()),
    }
}

/// Used/total capacity ratio for sorting; clusters without cached capacity sort
/// as 0.
fn usage_ratio(item: &CephClusterListEntry) -> f64 {
    match (item.bytes_used, item.bytes_total) {
        (Some(used), Some(total)) if total > 0 => used as f64 / total as f64,
        _ => 0.0,
    }
}

async fn load_clusters() -> Result<Vec<CephClusterListEntry>, Error> {
    proxmox_yew_comp::http_get("/ceph/clusters", None).await
}

#[derive(PartialEq, Properties)]
pub struct CephClusterListPanel {
    /// Invoked with the chosen cluster when a row is activated (double-click).
    #[prop_or_default]
    on_select: Option<Callback<CephClusterListEntry>>,
}

impl CephClusterListPanel {
    pub fn new() -> Self {
        yew::props!(Self {})
    }

    /// Set the callback fired when the user activates a cluster row.
    pub fn on_select(mut self, cb: Callback<CephClusterListEntry>) -> Self {
        self.on_select = Some(cb);
        self
    }
}

pub struct PdmCephClusterListPanel {
    state: LoadableComponentState<()>,
    selection: Selection,
    store: Store<CephClusterListEntry>,
    columns: Rc<Vec<DataTableHeader<CephClusterListEntry>>>,
}

pwt::impl_deref_mut_property!(PdmCephClusterListPanel, state, LoadableComponentState<()>);

impl LoadableComponent for PdmCephClusterListPanel {
    type Message = ();
    type Properties = CephClusterListPanel;
    type ViewState = ();

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        // The list overlays cached health and never fetches live, so most polls
        // are served from the cache.
        ctx.link().repeated_load(10_000);
        let store =
            Store::with_extract_key(|item: &CephClusterListEntry| Key::from(item.cluster.clone()));

        // Single-click selection drives the master/detail split: forward the
        // selected cluster to the parent view.
        let selection = {
            let store = store.clone();
            let on_select = ctx.props().on_select.clone();
            Selection::new().on_select(move |selection: Selection| {
                if let Some(cb) = &on_select {
                    if let Some(entry) = store.selected_record(&selection) {
                        cb.emit(entry);
                    }
                }
            })
        };

        Self {
            state: LoadableComponentState::new(),
            selection,
            store,
            columns: columns(),
        }
    }

    fn load(
        &self,
        _ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        let store = self.store.clone();
        Box::pin(async move {
            // Resolve the data before taking the store's write borrow: holding
            // the guard across the await lets a concurrent render read the same
            // store and panics with "RefCell already borrowed".
            let data = load_clusters().await?;
            store.write().set_data(data);
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
        DataTable::new(Rc::clone(&self.columns), self.store.clone())
            .class(pwt::css::FlexFit)
            .selection(self.selection.clone())
            .into()
    }
}

impl From<CephClusterListPanel> for VNode {
    fn from(val: CephClusterListPanel) -> Self {
        VNode::from(VComp::new::<LoadableComponentMaster<PdmCephClusterListPanel>>(
            Rc::new(val),
            None,
        ))
    }
}

fn columns() -> Rc<Vec<DataTableHeader<CephClusterListEntry>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Health"))
            .width("95px")
            .render(|item: &CephClusterListEntry| {
                let (status, label) = health_cell(item);
                Row::new()
                    .gap(2)
                    .class(AlignItems::Center)
                    .with_child(Fa::from(status))
                    .with_child(html! { { label } })
                    .into()
            })
            // Default-sort worst-first so problem clusters surface at the top.
            .sorter(|a: &CephClusterListEntry, b: &CephClusterListEntry| {
                row_severity(a).cmp(&row_severity(b))
            })
            .sort_order(false)
            .into(),
        DataTableColumn::new(tr!("Name"))
            .flex(1)
            .get_property(|item: &CephClusterListEntry| &item.display_name)
            .into(),
        DataTableColumn::new(tr!("Remote"))
            .width("110px")
            .render(|item: &CephClusterListEntry| {
                html! { { item.remote.clone().unwrap_or_default() } }
            })
            .into(),
        DataTableColumn::new(tr!("Usage"))
            .width("210px")
            .render(|item: &CephClusterListEntry| match (item.bytes_used, item.bytes_total) {
                (Some(used), Some(total)) if total > 0 => {
                    let pct = used as f64 / total as f64 * 100.0;
                    // 1-decimal byte precision keeps the cell compact enough to
                    // fit the cluster-list pane (e.g. "94.5 GiB", not "94.547").
                    usage_cell(
                        tr!(
                            "{0}% ({1} of {2})",
                            format!("{pct:.1}"),
                            format!("{:.1}", HumanByte::from(used.max(0) as u64)),
                            format!("{:.1}", HumanByte::from(total.max(0) as u64))
                        ),
                        pct,
                    )
                }
                _ => html! { {"-"} },
            })
            .sorter(|a: &CephClusterListEntry, b: &CephClusterListEntry| {
                usage_ratio(a).total_cmp(&usage_ratio(b))
            })
            .into(),
        DataTableColumn::new(tr!("Problems"))
            .width("100px")
            .justify("right")
            .render(|item: &CephClusterListEntry| match item.problem_count {
                Some(n) if n > 0 => Container::new()
                    .class(FontColor::Warning)
                    .with_child(n.to_string())
                    .into(),
                Some(n) => html! { { n.to_string() } },
                None => html! { {"-"} },
            })
            .sorter(|a: &CephClusterListEntry, b: &CephClusterListEntry| {
                a.problem_count.unwrap_or(0).cmp(&b.problem_count.unwrap_or(0))
            })
            .into(),
    ])
}
