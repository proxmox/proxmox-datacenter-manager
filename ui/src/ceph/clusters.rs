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
use pwt::widget::{Button, Container, Fa, Meter, Row, Toolbar};

use proxmox_yew_comp::{
    LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState, Status,
};

use pdm_api_types::ceph::{CephClusterListEntry, CephClusterState};

use super::renderer::{
    ceph_cluster_state_label, ceph_health_label, ceph_health_severity, ceph_health_status,
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

/// A daemon "up / total" cell (OSDs up, monitors in quorum). Error-colored when
/// fewer are up than exist, since a down OSD or an out-of-quorum monitor is an
/// availability risk that should stand out; "-" when uncached.
fn daemon_cell(up: Option<i64>, total: Option<i64>) -> Html {
    match (up, total) {
        (Some(up), Some(total)) => {
            let text = format!("{up} / {total}");
            if up < total {
                Container::new()
                    .class(FontColor::Error)
                    .with_child(text)
                    .into()
            } else {
                html! { { text } }
            }
        }
        _ => html! { {"-"} },
    }
}

/// An OSD "up / in / total" cell. Red when any OSD is down (data availability
/// risk), amber when all are up but some are out (drained/rebalancing).
fn osd_cell(up: Option<i64>, in_cluster: Option<i64>, total: Option<i64>) -> Html {
    match (up, in_cluster, total) {
        (Some(up), Some(in_cluster), Some(total)) => {
            let text = format!("{up} / {in_cluster} / {total}");
            let color = if up < total {
                Some(FontColor::Error)
            } else if in_cluster < total {
                Some(FontColor::Warning)
            } else {
                None
            };
            match color {
                Some(color) => Container::new().class(color).with_child(text).into(),
                None => html! { { text } },
            }
        }
        _ => html! { {"-"} },
    }
}

/// The most significant ongoing activity for triage: storage pressure outranks
/// reduced redundancy, which outranks transient recovery; blank when idle.
fn activity_cell(item: &CephClusterListEntry) -> Html {
    let (label, color) = if item.nearfull == Some(true) {
        (tr!("Near full"), FontColor::Error)
    } else if item.degraded == Some(true) {
        (tr!("Degraded"), FontColor::Warning)
    } else if item.recovering == Some(true) {
        (tr!("Recovering"), FontColor::Warning)
    } else {
        return html! { {"-"} };
    };
    Container::new().class(color).with_child(label).into()
}

/// A usage cell: a threshold-colored fill bar plus the percentage.
fn usage_meter(used: i64, total: i64) -> Html {
    let ratio = if total > 0 {
        (used as f64 / total as f64) as f32
    } else {
        0.0
    };
    Row::new()
        .gap(2)
        .class(AlignItems::Center)
        // Let the meter grow to fill the cell; the percentage text keeps its content width, so the
        // bar takes whatever is left.
        .with_child(
            Container::new().style("flex", "1").with_child(
                Meter::new()
                    .low(0.7)
                    .high(0.85)
                    .optimum(0.0)
                    .value(ratio)
                    .animated(true)
                    .width("100%"),
            ),
        )
        .with_child(html! { { format!("{:.1}%", ratio * 100.0) } })
        .into()
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
        VNode::from(
            VComp::new::<LoadableComponentMaster<PdmCephClusterListPanel>>(Rc::new(val), None),
        )
    }
}

fn columns() -> Rc<Vec<DataTableHeader<CephClusterListEntry>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Health"))
            .width("minmax(80px,2fr)")
            .render(|item: &CephClusterListEntry| {
                let (status, mut label) = health_cell(item);
                // Fold the active health-check count into the label rather than
                // a separate column that just restates the icon.
                if let Some(n) = item.problem_count {
                    if n > 0 {
                        label = tr!("{0} ({1})", label, n);
                    }
                }
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
            .width("minmax(125px,3fr)")
            .get_property(|item: &CephClusterListEntry| &item.display_name)
            .into(),
        DataTableColumn::new(tr!("Remote"))
            .width("minmax(100px,3fr)")
            .render(|item: &CephClusterListEntry| {
                html! { { item.remote.clone().unwrap_or_default() } }
            })
            .into(),
        DataTableColumn::new(tr!("Capacity"))
            .width("minmax(75px,1fr)")
            .justify("right")
            .render(|item: &CephClusterListEntry| match item.bytes_total {
                Some(total) if total > 0 => {
                    html! { { format!("{:.1}", HumanByte::from(total.max(0) as u64)) } }
                }
                _ => html! { {"-"} },
            })
            .sorter(|a: &CephClusterListEntry, b: &CephClusterListEntry| {
                a.bytes_total.unwrap_or(0).cmp(&b.bytes_total.unwrap_or(0))
            })
            .into(),
        DataTableColumn::new(tr!("Available"))
            .width("minmax(75px,1fr)")
            .justify("right")
            .render(|item: &CephClusterListEntry| match item.bytes_avail {
                Some(avail) => html! { { format!("{:.1}", HumanByte::from(avail.max(0) as u64)) } },
                None => html! { {"-"} },
            })
            .sorter(|a: &CephClusterListEntry, b: &CephClusterListEntry| {
                a.bytes_avail.unwrap_or(0).cmp(&b.bytes_avail.unwrap_or(0))
            })
            .into(),
        DataTableColumn::new(tr!("Usage"))
            .width("minmax(100px,3fr)")
            .render(
                |item: &CephClusterListEntry| match (item.bytes_used, item.bytes_total) {
                    (Some(used), Some(total)) if total > 0 => usage_meter(used, total),
                    _ => html! { {"-"} },
                },
            )
            .sorter(|a: &CephClusterListEntry, b: &CephClusterListEntry| {
                usage_ratio(a).total_cmp(&usage_ratio(b))
            })
            .into(),
        DataTableColumn::new(tr!("OSDs"))
            .width("minmax(100px,2fr)")
            .justify("right")
            .render(|item: &CephClusterListEntry| {
                osd_cell(item.osds_up, item.osds_in, item.osds_total)
            })
            .sorter(|a: &CephClusterListEntry, b: &CephClusterListEntry| {
                a.osds_up.unwrap_or(0).cmp(&b.osds_up.unwrap_or(0))
            })
            .into(),
        DataTableColumn::new(tr!("Monitors"))
            .width("minmax(75px,2fr)")
            .justify("right")
            .render(|item: &CephClusterListEntry| daemon_cell(item.mons_in_quorum, item.mons_total))
            .sorter(|a: &CephClusterListEntry, b: &CephClusterListEntry| {
                a.mons_in_quorum
                    .unwrap_or(0)
                    .cmp(&b.mons_in_quorum.unwrap_or(0))
            })
            .into(),
        DataTableColumn::new(tr!("Activity"))
            .width("minmax(50px,2fr)")
            .render(activity_cell)
            .into(),
    ])
}
