//! Ceph OSDs tab: the CRUSH/OSD tree from `GET /ceph/clusters/{id}/osd-tree`.
//!
//! The endpoint returns the raw PVE `ceph osd tree` object (`{root, flags}`),
//! a recursive bucket tree (root -> host -> osd). We parse it defensively into
//! a [`TreeStore`] so the panel survives Ceph-release shape changes, and render
//! a host/OSD tree with status, device class, usage and latency.

use std::cmp::Ordering;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;
use serde_json::Value;

use yew::virtual_dom::{Key, VComp, VNode};

use pwt::css::{AlignItems, FontColor};
use pwt::prelude::*;
use pwt::props::ExtractPrimaryKey;
use pwt::state::{SlabTree, Store, TreeStore};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::{Button, Container, Fa, Row, Toolbar};

use proxmox_yew_comp::{
    LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState, Status,
};

use super::renderer::usage_pct_cell;
use crate::renderer::render_tree_column;

/// A single OSD's display data, flattened from the tree node.
#[derive(Clone, PartialEq, Debug)]
struct OsdData {
    name: String,
    /// Name of the enclosing `host` CRUSH bucket. Empty in the tree view (the
    /// hierarchy conveys it); set when flattened for the flat view.
    host: String,
    status: String,
    in_cluster: bool,
    device_class: Option<String>,
    pgs: i64,
    percent_used: f64,
    bytes_used: i64,
    total_space: i64,
    apply_latency_ms: i64,
    commit_latency_ms: i64,
}

#[derive(Clone, PartialEq, Debug)]
enum OsdTreeEntry {
    Root,
    /// A CRUSH bucket (the `default` root, a host, a rack, ...).
    Bucket {
        id: String,
        name: String,
        kind: String,
    },
    Osd(OsdData),
}

impl OsdTreeEntry {
    fn sort_name(&self) -> &str {
        match self {
            OsdTreeEntry::Root => "",
            OsdTreeEntry::Bucket { name, .. } => name,
            OsdTreeEntry::Osd(o) => &o.name,
        }
    }
}

impl ExtractPrimaryKey for OsdTreeEntry {
    fn extract_key(&self) -> Key {
        Key::from(match self {
            OsdTreeEntry::Root => "/".to_string(),
            OsdTreeEntry::Bucket { id, .. } => format!("b{id}"),
            OsdTreeEntry::Osd(o) => format!("osd/{}", o.name),
        })
    }
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(String::from)
}

/// Read a field as a string, accepting either a JSON string or number. CRUSH
/// bucket ids come through as strings in current PVE output but are integers in
/// raw `ceph osd tree`; tolerate both so bucket keys stay unique either way.
fn id_field(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn i64_field(v: &Value, key: &str) -> i64 {
    v.get(key)
        .and_then(|x| x.as_i64().or_else(|| x.as_f64().map(|f| f as i64)))
        .unwrap_or(0)
}

fn f64_field(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(|x| x.as_f64()).unwrap_or(0.0)
}

fn parse_osd(node: &Value) -> OsdData {
    OsdData {
        name: str_field(node, "name").unwrap_or_default(),
        host: String::new(),
        status: str_field(node, "status").unwrap_or_default(),
        in_cluster: i64_field(node, "in") != 0,
        device_class: str_field(node, "device_class"),
        pgs: i64_field(node, "pgs"),
        percent_used: f64_field(node, "percent_used"),
        bytes_used: i64_field(node, "bytes_used"),
        total_space: i64_field(node, "total_space"),
        apply_latency_ms: i64_field(node, "apply_latency_ms"),
        commit_latency_ms: i64_field(node, "commit_latency_ms"),
    }
}

/// Append a parsed tree node (and its children) under `parent`.
fn append_node(parent: &mut pwt::state::SlabTreeNodeMut<'_, OsdTreeEntry>, node: &Value) {
    let kind = str_field(node, "type").unwrap_or_default();
    if kind == "osd" {
        parent.append(OsdTreeEntry::Osd(parse_osd(node)));
        return;
    }
    let entry = OsdTreeEntry::Bucket {
        id: id_field(node, "id"),
        name: str_field(node, "name").unwrap_or_default(),
        kind,
    };
    let mut child = parent.append(entry);
    child.set_expanded(true);
    if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
        for c in children {
            append_node(&mut child, c);
        }
    }
}

fn build_tree(raw: &Value) -> SlabTree<OsdTreeEntry> {
    let mut tree = SlabTree::new();
    let mut root = tree.set_root(OsdTreeEntry::Root);
    root.set_expanded(true);
    // `root.children` holds the top-level CRUSH buckets (typically a single
    // `default` root bucket); recurse from there.
    if let Some(buckets) = raw
        .get("root")
        .and_then(|r| r.get("children"))
        .and_then(|c| c.as_array())
    {
        for b in buckets {
            append_node(&mut root, b);
        }
    }
    tree
}

/// Flatten the OSD tree into a flat list, tagging each OSD with its enclosing
/// host. Lets the flat view sort every OSD by latency / usage across all hosts,
/// which the tree view (sorting only siblings) cannot.
fn flatten_osds(raw: &Value) -> Vec<OsdData> {
    fn collect(node: &Value, host: &str, out: &mut Vec<OsdData>) {
        let kind = str_field(node, "type").unwrap_or_default();
        if kind == "osd" {
            let mut osd = parse_osd(node);
            osd.host = host.to_string();
            out.push(osd);
            return;
        }
        // Track the nearest enclosing `host` bucket as the OSD's host label.
        let host = if kind == "host" {
            str_field(node, "name").unwrap_or_default()
        } else {
            host.to_string()
        };
        if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
            for child in children {
                collect(child, &host, out);
            }
        }
    }

    let mut out = Vec::new();
    if let Some(buckets) = raw
        .get("root")
        .and_then(|r| r.get("children"))
        .and_then(|c| c.as_array())
    {
        for bucket in buckets {
            collect(bucket, "", &mut out);
        }
    }
    out
}

/// Status icon + "up / in" text for one OSD. Shared by the tree and flat views.
fn osd_status_cell(o: &OsdData) -> Html {
    let up = o.status == "up";
    let status = if !up {
        Status::Error
    } else if !o.in_cluster {
        Status::Warning
    } else {
        Status::Success
    };
    let status_text = if up { tr!("up") } else { tr!("down") };
    let membership = if o.in_cluster { tr!("in") } else { tr!("out") };
    let text = format!("{status_text} / {membership}");
    Row::new()
        .gap(2)
        .class(AlignItems::Center)
        .with_child(Fa::from(status))
        .with_child(text)
        .into()
}

/// Apply/commit latency cell, colored only when slow. Shared by both views.
fn osd_latency_cell(o: &OsdData) -> Html {
    let max = o.apply_latency_ms.max(o.commit_latency_ms);
    let text = tr!("{0} / {1} ms", o.apply_latency_ms, o.commit_latency_ms);
    let color = if max >= 100 {
        Some(FontColor::Error)
    } else if max >= 30 {
        Some(FontColor::Warning)
    } else {
        None
    };
    match color {
        Some(color) => Container::new().class(color).with_child(text).into(),
        None => html! { { text } },
    }
}

async fn load_tree(cluster: &str) -> Result<Value, Error> {
    proxmox_yew_comp::http_get(format!("/ceph/clusters/{cluster}/osd-tree"), None).await
}

#[derive(PartialEq, Properties)]
pub struct CephOsdsPanel {
    cluster: AttrValue,
}

impl CephOsdsPanel {
    pub fn new(cluster: impl Into<AttrValue>) -> Self {
        yew::props!(Self {
            cluster: cluster.into()
        })
    }
}

pub enum Msg {
    /// Switch between the host/OSD tree and the flat, globally-sortable list.
    ToggleFlat,
}

pub struct PdmCephOsdsPanel {
    state: LoadableComponentState<()>,
    store: TreeStore<OsdTreeEntry>,
    columns: Rc<Vec<DataTableHeader<OsdTreeEntry>>>,
    /// When set, render the flat list instead of the tree.
    flat: bool,
    flat_store: Store<OsdData>,
    flat_columns: Rc<Vec<DataTableHeader<OsdData>>>,
}

pwt::impl_deref_mut_property!(PdmCephOsdsPanel, state, LoadableComponentState<()>);

fn default_sorter(a: &OsdTreeEntry, b: &OsdTreeEntry) -> Ordering {
    a.sort_name().cmp(b.sort_name())
}

impl LoadableComponent for PdmCephOsdsPanel {
    type Message = Msg;
    type Properties = CephOsdsPanel;
    type ViewState = ();

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        ctx.link().repeated_load(15_000);
        let store = TreeStore::new().view_root(false);
        store.set_sorter(default_sorter);
        Self {
            state: LoadableComponentState::new(),
            columns: columns(store.clone()),
            store,
            flat: false,
            flat_store: Store::with_extract_key(|o: &OsdData| Key::from(o.name.clone())),
            flat_columns: flat_columns(),
        }
    }

    fn update(&mut self, _ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::ToggleFlat => {
                self.flat = !self.flat;
                true
            }
        }
    }

    fn load(
        &self,
        ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        let cluster = ctx.props().cluster.clone();
        let store = self.store.clone();
        let flat_store = self.flat_store.clone();
        Box::pin(async move {
            // Resolve before taking either store's write borrow (a guard held
            // across an await panics with "RefCell already borrowed").
            let raw = load_tree(&cluster).await?;
            let tree = build_tree(&raw);
            let flat = flatten_osds(&raw);
            store.write().update_root_tree(tree);
            flat_store.write().set_data(flat);
            Ok(())
        })
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        let link = ctx.link().clone();
        let loading = self.loading();
        let toggle_link = ctx.link().clone();
        let (toggle_label, toggle_icon) = if self.flat {
            (tr!("Tree view"), "fa fa-sitemap")
        } else {
            (tr!("Flat view"), "fa fa-list")
        };
        Some(
            Toolbar::new()
                .class("pwt-overflow-hidden")
                .class("pwt-border-bottom")
                .with_flex_spacer()
                .with_child(
                    Button::new(toggle_label)
                        .icon_class(toggle_icon)
                        .onclick(move |_| toggle_link.send_message(Msg::ToggleFlat)),
                )
                .with_child(Button::refresh(loading).onclick(move |_| link.send_reload()))
                .into(),
        )
    }

    fn main_view(&self, _ctx: &LoadableComponentContext<Self>) -> Html {
        if self.flat {
            DataTable::new(Rc::clone(&self.flat_columns), self.flat_store.clone())
                .class(pwt::css::FlexFit)
                .into()
        } else {
            DataTable::new(Rc::clone(&self.columns), self.store.clone())
                .class(pwt::css::FlexFit)
                .into()
        }
    }
}

impl From<CephOsdsPanel> for VNode {
    fn from(val: CephOsdsPanel) -> Self {
        VNode::from(VComp::new::<LoadableComponentMaster<PdmCephOsdsPanel>>(
            Rc::new(val),
            None,
        ))
    }
}

// Sort keys below sort OSDs within their host (siblings in the tree); buckets
// return a sentinel so hosts keep their default order regardless of the column.

/// Status severity for sorting: down > out > healthy. Non-OSD rows sort lowest.
fn osd_status_rank(e: &OsdTreeEntry) -> i64 {
    match e {
        OsdTreeEntry::Osd(o) if o.status != "up" => 2,
        OsdTreeEntry::Osd(o) if !o.in_cluster => 1,
        OsdTreeEntry::Osd(_) => 0,
        _ => -1,
    }
}

fn osd_percent_used(e: &OsdTreeEntry) -> f64 {
    match e {
        OsdTreeEntry::Osd(o) => o.percent_used,
        _ => -1.0,
    }
}

fn osd_pgs(e: &OsdTreeEntry) -> i64 {
    match e {
        OsdTreeEntry::Osd(o) => o.pgs,
        _ => -1,
    }
}

fn osd_max_latency(e: &OsdTreeEntry) -> i64 {
    match e {
        OsdTreeEntry::Osd(o) => o.apply_latency_ms.max(o.commit_latency_ms),
        _ => -1,
    }
}

fn columns(store: TreeStore<OsdTreeEntry>) -> Rc<Vec<DataTableHeader<OsdTreeEntry>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Name / Host"))
            .tree_column(store)
            .flex(2)
            .render(|e: &OsdTreeEntry| {
                let (icon, text) = match e {
                    OsdTreeEntry::Root => ("sitemap", String::new()),
                    OsdTreeEntry::Bucket { name, kind, .. } => {
                        let icon = if kind == "host" { "building" } else { "sitemap" };
                        (icon, name.clone())
                    }
                    OsdTreeEntry::Osd(o) => ("hdd-o", o.name.clone()),
                };
                // Use the shared tree-cell helper so long names get min-width:0
                // + ellipsis instead of being clipped to a single glyph.
                render_tree_column(Fa::new(icon).into(), text).into()
            })
            .sorter(default_sorter)
            .into(),
        DataTableColumn::new(tr!("Status"))
            .width("120px")
            .render(|e: &OsdTreeEntry| match e {
                OsdTreeEntry::Osd(o) => osd_status_cell(o),
                _ => html! {},
            })
            .sorter(|a: &OsdTreeEntry, b: &OsdTreeEntry| {
                osd_status_rank(a).cmp(&osd_status_rank(b))
            })
            .into(),
        DataTableColumn::new(tr!("Class"))
            .width("80px")
            .render(|e: &OsdTreeEntry| match e {
                OsdTreeEntry::Osd(o) => html! { { o.device_class.clone().unwrap_or_default() } },
                _ => html! {},
            })
            .into(),
        DataTableColumn::new(tr!("Used"))
            .width("90px")
            .justify("right")
            .render(|e: &OsdTreeEntry| match e {
                // Percentage only: the per-OSD byte sizes don't fit the (often
                // half-width) detail pane, and % is the key per-disk fullness
                // signal. Threshold-colored above 70/85%, neutral below.
                OsdTreeEntry::Osd(o) => usage_pct_cell(o.percent_used),
                _ => html! {},
            })
            .sorter(|a: &OsdTreeEntry, b: &OsdTreeEntry| {
                osd_percent_used(a).total_cmp(&osd_percent_used(b))
            })
            .into(),
        DataTableColumn::new(tr!("PGs"))
            .width("70px")
            .justify("right")
            .render(|e: &OsdTreeEntry| match e {
                OsdTreeEntry::Osd(o) => html! { { o.pgs.to_string() } },
                _ => html! {},
            })
            .sorter(|a: &OsdTreeEntry, b: &OsdTreeEntry| osd_pgs(a).cmp(&osd_pgs(b)))
            .into(),
        DataTableColumn::new(tr!("Latency"))
            .width("120px")
            .justify("right")
            .render(|e: &OsdTreeEntry| match e {
                OsdTreeEntry::Osd(o) => osd_latency_cell(o),
                _ => html! {},
            })
            .sorter(|a: &OsdTreeEntry, b: &OsdTreeEntry| {
                osd_max_latency(a).cmp(&osd_max_latency(b))
            })
            .into(),
    ])
}

/// Columns for the flat OSD view: every OSD as one row with a `Host` column,
/// so any column sorts globally across hosts (the tree only sorts siblings).
fn flat_columns() -> Rc<Vec<DataTableHeader<OsdData>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Name"))
            .flex(1)
            .render(|o: &OsdData| {
                render_tree_column(Fa::new("hdd-o").into(), o.name.clone()).into()
            })
            .sorter(|a: &OsdData, b: &OsdData| a.name.cmp(&b.name))
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Host"))
            .width("140px")
            .render(|o: &OsdData| html! { { o.host.clone() } })
            .sorter(|a: &OsdData, b: &OsdData| a.host.cmp(&b.host))
            .into(),
        DataTableColumn::new(tr!("Status"))
            .width("120px")
            .render(osd_status_cell)
            .sorter(|a: &OsdData, b: &OsdData| flat_status_rank(a).cmp(&flat_status_rank(b)))
            .into(),
        DataTableColumn::new(tr!("Class"))
            .width("80px")
            .render(|o: &OsdData| html! { { o.device_class.clone().unwrap_or_default() } })
            .into(),
        DataTableColumn::new(tr!("Used"))
            .width("90px")
            .justify("right")
            .render(|o: &OsdData| usage_pct_cell(o.percent_used))
            .sorter(|a: &OsdData, b: &OsdData| a.percent_used.total_cmp(&b.percent_used))
            .into(),
        DataTableColumn::new(tr!("PGs"))
            .width("70px")
            .justify("right")
            .render(|o: &OsdData| html! { { o.pgs.to_string() } })
            .sorter(|a: &OsdData, b: &OsdData| a.pgs.cmp(&b.pgs))
            .into(),
        DataTableColumn::new(tr!("Latency"))
            .width("120px")
            .justify("right")
            .render(osd_latency_cell)
            .sorter(|a: &OsdData, b: &OsdData| {
                a.apply_latency_ms
                    .max(a.commit_latency_ms)
                    .cmp(&b.apply_latency_ms.max(b.commit_latency_ms))
            })
            .into(),
    ])
}

/// Status severity for sorting the flat view: down > out > healthy.
fn flat_status_rank(o: &OsdData) -> i64 {
    if o.status != "up" {
        2
    } else if !o.in_cluster {
        1
    } else {
        0
    }
}
