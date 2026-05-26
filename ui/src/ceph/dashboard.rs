//! Per-cluster Ceph dashboard: health, capacity and service overview.
//!
//! Binds `GET /ceph/clusters/{cluster}/summary` (the server-summarized [`CephClusterStatus`]) so
//! the panel reads typed fields rather than parsing the raw `ceph status` blob.
//!
//! Layout follows the PVE remote-overview convention (section title rows + separators + meter
//! rows), not bordered sub-panels, so it matches the rest of PDM's detail panes.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;

use yew::virtual_dom::{VComp, VNode};

use proxmox_human_byte::HumanByte;

use pwt::css::{AlignItems, ColorScheme, FlexFit};
use pwt::prelude::*;
use pwt::widget::{Button, Column, Container, Fa, Row, Toolbar};

use proxmox_yew_comp::{
    LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState, MeterLabel, Status,
};

use pdm_api_types::ceph::{CephClusterStatus, CephHealthCheck};

use super::renderer::{ceph_health_label, ceph_health_status, usage_cell};
use crate::renderer::{render_title_row, separator};

async fn load_summary(cluster: &str) -> Result<CephClusterStatus, Error> {
    proxmox_yew_comp::http_get(format!("/ceph/clusters/{cluster}/summary"), None).await
}

#[derive(PartialEq, Properties)]
pub struct CephDashboardPanel {
    cluster: AttrValue,
}

impl CephDashboardPanel {
    pub fn new(cluster: impl Into<AttrValue>) -> Self {
        yew::props!(Self {
            cluster: cluster.into()
        })
    }
}

pub struct PdmCephDashboardPanel {
    state: LoadableComponentState<()>,
    status: Rc<RefCell<Option<CephClusterStatus>>>,
}

pwt::impl_deref_mut_property!(PdmCephDashboardPanel, state, LoadableComponentState<()>);

impl LoadableComponent for PdmCephDashboardPanel {
    type Message = ();
    type Properties = CephDashboardPanel;
    type ViewState = ();

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        // The summary is cached server-side; polling mostly hits that cache.
        ctx.link().repeated_load(10_000);
        Self {
            state: LoadableComponentState::new(),
            status: Rc::new(RefCell::new(None)),
        }
    }

    fn load(
        &self,
        ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        let cluster = ctx.props().cluster.clone();
        let status = self.status.clone();
        Box::pin(async move {
            // Resolve before taking the borrow; never hold it across the await.
            let data = load_summary(&cluster).await?;
            *status.borrow_mut() = Some(data);
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
        let guard = self.status.borrow();
        let Some(status) = guard.as_ref() else {
            return Container::new().class(FlexFit).into();
        };

        let mut col = Column::new().class(FlexFit).padding(4).gap(2);

        // Health: humanized state, a plain-language data-at-risk line, then the active checks (the
        // things actually needing attention).
        col = col.with_child(render_title_row(tr!("Health"), "heartbeat"));
        col = col.with_child(health_line(status));
        col = col.with_child(redundancy_line(status));
        if let Some(row) = recovery_line(status) {
            col = col.with_child(row);
        }
        for check in &status.checks {
            col = col.with_child(check_row(check));
        }

        // Capacity: a threshold-colored meter (green < 70%, amber < 85%, red).
        col = col.with_child(separator());
        col = col.with_child(render_title_row(tr!("Capacity"), "database"));
        col = col.with_child(capacity_row(status));
        // Surface the most-utilized pool: a near-full pool can block writes long before the
        // cluster-wide figure looks alarming.
        if let Some(row) = fullest_pool_line(status) {
            col = col.with_child(row);
        }

        // Services: counts. PG breakdown is folded into one derived line rather than a noisy
        // per-state table.
        col = col.with_child(separator());
        col = col.with_child(render_title_row(tr!("Services"), "server"));
        col = col.with_child(kv(
            tr!("Monitors"),
            tr!(
                "{0} / {1} in quorum",
                status.mons_in_quorum,
                status.mons_total
            ),
        ));
        col = col.with_child(kv(tr!("Managers"), mgr_text(status)));
        col = col.with_child(kv(
            tr!("OSDs"),
            tr!(
                "{0} up, {1}/{2} in",
                status.osds_up,
                status.osds_in,
                status.osds_total
            ),
        ));
        col = col.with_child(kv(tr!("Pools"), status.num_pools.to_string()));
        col = col.with_child(kv(tr!("Placement groups"), pg_summary(status)));
        if let Some(version) = &status.version {
            let text = if status.version_mixed {
                tr!("{0} (mixed versions)", version)
            } else {
                version.clone()
            };
            col = col.with_child(kv(tr!("Ceph version"), text));
        }

        // Members: which PVE remotes (and their nodes) back this cluster - the PDM registry view,
        // surfacing cross-remote membership at a glance.
        let member_rows = members_rows(status);
        if !member_rows.is_empty() {
            col = col.with_child(separator());
            col = col.with_child(render_title_row(tr!("Members"), "sitemap"));
            col = member_rows.into_iter().fold(col, |c, r| c.with_child(r));
        }

        // Performance: only when the cluster actually reports client/recovery activity, so an idle
        // cluster is not padded with zeros.
        if let Some(rows) = perf_rows(status) {
            col = col.with_child(separator());
            col = col.with_child(render_title_row(tr!("Performance"), "exchange"));
            col = rows.into_iter().fold(col, |c, r| c.with_child(r));
        }

        Container::new()
            .class(FlexFit)
            .class(ColorScheme::Neutral)
            .with_child(col)
            .into()
    }
}

impl From<CephDashboardPanel> for VNode {
    fn from(val: CephDashboardPanel) -> Self {
        VNode::from(
            VComp::new::<LoadableComponentMaster<PdmCephDashboardPanel>>(Rc::new(val), None),
        )
    }
}

/// A fixed-width-label key/value row.
fn kv(label: String, value: String) -> Row {
    Row::new()
        .gap(2)
        .class(AlignItems::Baseline)
        .with_child(Container::new().width("160px").with_child(label))
        .with_child(Container::new().with_child(value))
}

/// One row per backing remote, listing its member nodes; standalone members (no remote) are grouped
/// under a "Standalone" row. Compact because the per-node daemon detail lives in the Monitors /
/// Managers / OSDs tabs - this is just the "which remotes host this cluster" overview.
fn members_rows(status: &CephClusterStatus) -> Vec<Row> {
    let mut by_remote: std::collections::BTreeMap<Option<String>, Vec<String>> =
        std::collections::BTreeMap::new();
    for member in &status.members {
        let node = member
            .node
            .clone()
            .or_else(|| member.remote.clone())
            .unwrap_or_default();
        by_remote
            .entry(member.remote.clone())
            .or_default()
            .push(node);
    }
    by_remote
        .into_iter()
        .map(|(remote, mut nodes)| {
            nodes.sort();
            kv(
                remote.unwrap_or_else(|| tr!("Standalone")),
                nodes.join(", "),
            )
        })
        .collect()
}

fn health_line(status: &CephClusterStatus) -> Row {
    // Humanized label only; the raw HEALTH_* token is redundant with the icon and the per-check
    // detail below, and is untranslated wire jargon.
    Row::new()
        .gap(2)
        .class(AlignItems::Center)
        .with_child(Fa::from(ceph_health_status(Some(&status.health))))
        .with_child(html! { { ceph_health_label(Some(&status.health)) } })
}

/// Plain-language "is my data at risk?" line: down/out OSDs, incomplete mon quorum, and degraded
/// objects. The single signal a manager escalates on.
fn redundancy_line(status: &CephClusterStatus) -> Row {
    let mut issues: Vec<String> = Vec::new();
    if status.osds_total > status.osds_up {
        issues.push(tr!("{0} OSD(s) down", status.osds_total - status.osds_up));
    }
    if status.osds_total > status.osds_in {
        issues.push(tr!("{0} OSD(s) out", status.osds_total - status.osds_in));
    }
    if status.mons_total > status.mons_in_quorum {
        issues.push(tr!(
            "monitor quorum {0}/{1}",
            status.mons_in_quorum,
            status.mons_total
        ));
    }
    if status.degraded_ratio.unwrap_or(0.0) > 0.0 {
        issues.push(tr!(
            "{0}% degraded",
            format!("{:.2}", status.degraded_ratio.unwrap_or(0.0) * 100.0)
        ));
    }

    let (icon, text) = if issues.is_empty() {
        (Status::Success, tr!("All data fully redundant"))
    } else if status.osds_total > status.osds_up || status.degraded_ratio.unwrap_or(0.0) > 0.0 {
        (Status::Error, issues.join(", "))
    } else {
        (Status::Warning, issues.join(", "))
    };

    Row::new()
        .gap(2)
        .class(AlignItems::Center)
        .with_child(Fa::from(icon))
        .with_child(html! { { tr!("Redundancy: {0}", text) } })
}

/// When the cluster is rebalancing/recovering, surface it explicitly: a warning state may be
/// self-healing rather than stuck, which changes how a manager reacts. Only shown while recovery is
/// actually happening.
fn recovery_line(status: &CephClusterStatus) -> Option<Row> {
    let recovery = status.recovery_bytes_sec.unwrap_or(0).max(0);
    let misplaced = status.misplaced_ratio.unwrap_or(0.0);
    if recovery == 0 && misplaced <= 0.0 {
        return None;
    }

    let mut parts: Vec<String> = Vec::new();
    if misplaced > 0.0 {
        parts.push(tr!("{0}% misplaced", format!("{:.2}", misplaced * 100.0)));
    }
    if recovery > 0 {
        parts.push(tr!("{0}/s recovery", HumanByte::from(recovery as u64)));
    }

    Some(
        Row::new()
            .gap(2)
            .class(AlignItems::Center)
            .with_child(Fa::from(Status::Warning))
            .with_child(html! { { tr!("Rebalancing: {0}", parts.join(", ")) } }),
    )
}

fn check_row(check: &CephHealthCheck) -> Row {
    let text = if check.summary.is_empty() {
        check.code.clone()
    } else {
        check.summary.clone()
    };
    Row::new()
        .gap(2)
        .class(AlignItems::Center)
        .with_child(Fa::from(ceph_health_status(Some(&check.severity))))
        .with_child(html! { { text } })
}

fn capacity_row(status: &CephClusterStatus) -> MeterLabel {
    let total = status.bytes_total.max(0) as u64;
    let used = status.bytes_used.max(0) as u64;
    let avail = status.bytes_avail.max(0) as u64;
    let ratio = if total == 0 {
        0.0
    } else {
        used as f64 / total as f64
    };
    // No icon: the "Capacity" section title already carries the database glyph, so a second one on
    // the meter row would be redundant.
    MeterLabel::with_zero_optimum(tr!("Usage"))
        .low(0.7)
        .high(0.85)
        .animated(true)
        .status(tr!(
            "{0}% ({1} of {2}, {3} free)",
            format!("{:.1}", ratio * 100.0),
            HumanByte::from(used),
            HumanByte::from(total),
            HumanByte::from(avail)
        ))
        .value(ratio as f32)
}

/// "Fullest pool" line for the capacity card, threshold-colored like the OSD and pool usage cells.
/// Absent when no pool usage is known.
fn fullest_pool_line(status: &CephClusterStatus) -> Option<Row> {
    let name = status.fullest_pool.clone()?;
    let used = status.fullest_pool_used.unwrap_or(0.0);
    Some(
        Row::new()
            .gap(2)
            .class(AlignItems::Baseline)
            .with_child(
                Container::new()
                    .width("160px")
                    .with_child(tr!("Fullest pool")),
            )
            .with_child(usage_cell(
                tr!("{0} ({1}%)", name, format!("{used:.1}")),
                used,
            )),
    )
}

fn mgr_text(status: &CephClusterStatus) -> String {
    match &status.mgr_active {
        Some(active) if status.mgr_standbys.is_empty() => active.clone(),
        Some(active) => tr!("{0} (+{1} standby)", active, status.mgr_standbys.len()),
        None => tr!("none"),
    }
}

fn pg_summary(status: &CephClusterStatus) -> String {
    let clean: i64 = status
        .pgs_by_state
        .iter()
        .filter(|g| g.state_name == "active+clean")
        .map(|g| g.count)
        .sum();
    if status.num_pgs > 0 && clean >= status.num_pgs {
        tr!("{0} (all active+clean)", status.num_pgs)
    } else {
        tr!(
            "{0} ({1} not active+clean)",
            status.num_pgs,
            status.num_pgs - clean
        )
    }
}

fn perf_rows(status: &CephClusterStatus) -> Option<Vec<Row>> {
    let mut rows: Vec<Row> = Vec::new();
    if let Some(v) = status.client_read_bytes_sec {
        rows.push(kv(
            tr!("Client read"),
            format!("{}/s", HumanByte::from(v.max(0) as u64)),
        ));
    }
    if let Some(v) = status.client_write_bytes_sec {
        rows.push(kv(
            tr!("Client write"),
            format!("{}/s", HumanByte::from(v.max(0) as u64)),
        ));
    }
    if let Some(v) = status.client_read_ops_sec {
        rows.push(kv(tr!("Client read ops"), tr!("{0}/s", v)));
    }
    if let Some(v) = status.client_write_ops_sec {
        rows.push(kv(tr!("Client write ops"), tr!("{0}/s", v)));
    }
    if let Some(v) = status.recovery_bytes_sec {
        rows.push(kv(
            tr!("Recovery"),
            format!("{}/s", HumanByte::from(v.max(0) as u64)),
        ));
    }
    if status.misplaced_ratio.unwrap_or(0.0) > 0.0 {
        rows.push(kv(
            tr!("Misplaced"),
            format!("{:.2}%", status.misplaced_ratio.unwrap_or(0.0) * 100.0),
        ));
    }

    if rows.is_empty() {
        None
    } else {
        Some(rows)
    }
}
