//! Central, cross-remote list of all guests (QEMU VMs and LXC containers).
//!
//! Provides a single filterable view over the guests of every remote PDM
//! manages, reusing the cached `/resources/list` aggregation. It can be shown as
//! a flat sortable table or as a tree grouped by remote, and offers the common
//! life-cycle actions (start, shutdown, migrate) plus a deep link into the
//! originating remote's web UI. It is currently surfaced as a tab in the
//! Remotes view.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;
use gloo_utils::window;
use serde::{Deserialize, Serialize};
use yew::virtual_dom::{Key, VComp, VNode};

use proxmox_human_byte::HumanByte;
use proxmox_yew_comp::utils::format_duration_human;
use proxmox_yew_comp::{
    rrd_value_renderer, LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScope, LoadableComponentScopeExt, LoadableComponentState,
};

use pwt::css::{AlignItems, ColorScheme, FlexFit, FontColor, FontStyle, JustifyContent};
use pwt::prelude::*;
use pwt::props::{
    ContainerBuilder, CssPaddingBuilder, ExtractPrimaryKey, StorageLocation, WidgetBuilder,
    WidgetStyleBuilder,
};
use pwt::state::{KeyedSlabTree, PersistentState, Selection, Store, TreeStore};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::Field;
use pwt::widget::{
    ActionIcon, Button, Column, Container, Fa, MessageBox, MessageBoxButtons, Row, SegmentedButton,
    Toolbar, Tooltip, Trigger,
};

use pdm_api_types::resource::{RemoteResources, Resource};
use pdm_api_types::RemoteUpid;

use crate::pve::utils::{guest_is_live, guest_status_label, render_guest_tags};
use crate::pve::{GuestInfo, GuestType};
use crate::renderer::{render_resource_name, render_status_icon, render_tree_column};
use crate::{get_deep_url, get_resource_node, widget::MigrateWindow};

/// Auto-reload interval for the cross-remote resource list.
const RELOAD_INTERVAL_MS: u32 = 10_000;

/// How the guest list is presented.
#[derive(Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum ViewMode {
    /// Flat, sortable table.
    #[default]
    Flat,
    /// Tree grouped by remote.
    Tree,
}

#[derive(Clone, PartialEq, Properties)]
pub struct GuestPanel {}

impl GuestPanel {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

impl Default for GuestPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl From<GuestPanel> for VNode {
    fn from(val: GuestPanel) -> Self {
        VComp::new::<LoadableComponentMaster<GuestPanelComp>>(Rc::new(val), None).into()
    }
}

/// One guest row: a guest resource together with the remote it lives on (the
/// remote is not part of [`Resource`] itself).
#[derive(Clone, PartialEq)]
struct GuestEntry {
    remote: String,
    resource: Resource,
}

impl GuestEntry {
    fn key(&self) -> Key {
        Key::from(self.resource.global_id())
    }

    fn guest_type(&self) -> GuestType {
        match &self.resource {
            Resource::PveLxc(_) => GuestType::Lxc,
            _ => GuestType::Qemu,
        }
    }

    fn vmid(&self) -> u32 {
        match &self.resource {
            Resource::PveQemu(r) => r.vmid,
            Resource::PveLxc(r) => r.vmid,
            _ => 0,
        }
    }

    fn template(&self) -> bool {
        match &self.resource {
            Resource::PveQemu(r) => r.template,
            Resource::PveLxc(r) => r.template,
            _ => false,
        }
    }

    fn cpu(&self) -> f64 {
        match &self.resource {
            Resource::PveQemu(r) => r.cpu,
            Resource::PveLxc(r) => r.cpu,
            _ => 0.0,
        }
    }

    fn mem(&self) -> u64 {
        match &self.resource {
            Resource::PveQemu(r) => r.mem,
            Resource::PveLxc(r) => r.mem,
            _ => 0,
        }
    }

    fn maxmem(&self) -> u64 {
        match &self.resource {
            Resource::PveQemu(r) => r.maxmem,
            Resource::PveLxc(r) => r.maxmem,
            _ => 0,
        }
    }

    fn uptime(&self) -> u64 {
        match &self.resource {
            Resource::PveQemu(r) => r.uptime,
            Resource::PveLxc(r) => r.uptime,
            _ => 0,
        }
    }

    fn tags(&self) -> &[String] {
        match &self.resource {
            Resource::PveQemu(r) => &r.tags,
            Resource::PveLxc(r) => &r.tags,
            _ => &[],
        }
    }

    fn node(&self) -> &str {
        get_resource_node(&self.resource).unwrap_or("")
    }

    fn guest_info(&self) -> GuestInfo {
        GuestInfo {
            guest_type: self.guest_type(),
            vmid: self.vmid(),
        }
    }
}

/// Tree node for the grouped-by-remote view.
#[derive(Clone, PartialEq)]
enum GuestTreeNode {
    Root,
    /// A remote group header, carrying its guest count for an at-a-glance summary.
    Remote(String, usize),
    Guest(GuestEntry),
}

impl ExtractPrimaryKey for GuestTreeNode {
    fn extract_key(&self) -> Key {
        match self {
            GuestTreeNode::Root => Key::from("__root__"),
            GuestTreeNode::Remote(name, _) => Key::from(format!("remote/{name}")),
            GuestTreeNode::Guest(entry) => entry.key(),
        }
    }
}

#[derive(PartialEq, Clone)]
pub enum Action {
    Start,
    Shutdown,
}

#[derive(PartialEq)]
pub enum ViewState {
    Confirm(Action, Key),
    Migrate(String, GuestInfo),
}

pub enum Msg {
    LoadFinished(Vec<RemoteResources>),
    Filter(String),
    SetViewMode(ViewMode),
    GuestAction(Action, Key),
    /// Show the progress of a started task, deriving the task base URL from the
    /// UPID's own remote so concurrent actions on different remotes can't clobber it.
    ShowTask(RemoteUpid),
}

#[doc(hidden)]
pub struct GuestPanelComp {
    state: LoadableComponentState<ViewState>,
    store: Store<GuestEntry>,
    tree_store: TreeStore<GuestTreeNode>,
    flat_columns: Rc<Vec<DataTableHeader<GuestEntry>>>,
    tree_columns: Rc<Vec<DataTableHeader<GuestTreeNode>>>,
    selection: Selection,
    filter: String,
    view_mode: PersistentState<ViewMode>,
    /// Whether the tree has been built at least once; the first build expands all
    /// remote groups, later rebuilds preserve the user's expand/collapse state.
    tree_built: bool,
    /// Number of remotes seen in the last load, to tell "no remotes" apart from
    /// "remotes present, but no guests" in the empty state.
    remote_count: usize,
    /// Remotes that could not be queried, surfaced as a non-blocking banner.
    failed_remotes: Vec<String>,
}

pwt::impl_deref_mut_property!(GuestPanelComp, state, LoadableComponentState<ViewState>);

impl GuestPanelComp {
    fn apply_filter(&self) {
        if self.filter.is_empty() {
            self.store.set_filter(None);
            self.tree_store.set_filter(None);
            return;
        }
        let text = self.filter.to_lowercase();
        let flat_text = text.clone();
        self.store
            .set_filter(move |entry: &GuestEntry| guest_matches(entry, &flat_text));
        self.tree_store
            .set_filter(move |node: &GuestTreeNode| match node {
                // keep remote group headers visible, filter only the guests
                GuestTreeNode::Guest(entry) => guest_matches(entry, &text),
                _ => true,
            });
    }
}

impl LoadableComponent for GuestPanelComp {
    type Properties = GuestPanel;
    type Message = Msg;
    type ViewState = ViewState;

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        ctx.link().repeated_load(RELOAD_INTERVAL_MS);

        // root stays hidden so the remote groups are the top-level rows
        let tree_store = TreeStore::new().view_root(false);

        Self {
            state: LoadableComponentState::new(),
            store: Store::with_extract_key(|entry: &GuestEntry| entry.key()),
            tree_columns: tree_columns(ctx.link().clone(), tree_store.clone()),
            tree_store,
            flat_columns: flat_columns(ctx.link().clone()),
            selection: Selection::new(),
            filter: String::new(),
            view_mode: PersistentState::new(StorageLocation::local("VirtualGuestsViewMode")),
            tree_built: false,
            remote_count: 0,
            failed_remotes: Vec::new(),
        }
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadFinished(remotes) => {
                self.remote_count = remotes.len();
                let mut entries = Vec::new();
                let mut failed = Vec::new();
                for remote_resources in remotes {
                    let RemoteResources {
                        remote,
                        error,
                        resources,
                    } = remote_resources;
                    if error.is_some() {
                        failed.push(remote.clone());
                    }
                    for resource in resources {
                        if matches!(resource, Resource::PveQemu(_) | Resource::PveLxc(_)) {
                            entries.push(GuestEntry {
                                remote: remote.clone(),
                                resource,
                            });
                        }
                    }
                }
                self.failed_remotes = failed;
                // only (re)build the tree when it is the active view; in flat mode
                // the work would be discarded. The filter is preserved across
                // set_data / update_root_tree, so it need not be reinstalled here.
                if *self.view_mode == ViewMode::Tree {
                    let expand = !self.tree_built;
                    self.tree_store
                        .write()
                        .update_root_tree(build_guest_tree(&entries, expand));
                    self.tree_built = true;
                }
                self.store.set_data(entries);
            }
            Msg::Filter(text) => {
                self.filter = text;
                self.apply_filter();
            }
            Msg::SetViewMode(mode) => {
                self.view_mode.update(mode);
                if mode == ViewMode::Tree {
                    // build the tree on demand from the current flat data
                    let expand = !self.tree_built;
                    let tree = build_guest_tree(self.store.read().data(), expand);
                    self.tree_store.write().update_root_tree(tree);
                    self.tree_built = true;
                }
            }
            Msg::ShowTask(upid) => {
                // derive the base URL from the UPID's own remote, so an action on
                // another remote that ran concurrently cannot point the task
                // viewer at the wrong remote
                self.set_task_base_url(format!("/pve/remotes/{}/tasks", upid.remote()).into());
                ctx.link().show_task_progres(upid.to_string());
            }
            Msg::GuestAction(action, key) => {
                let Some(entry) = self.store.read().lookup_record(&key).cloned() else {
                    return false;
                };
                let remote = entry.remote.clone();
                let node = entry.node().to_string();
                let vmid = entry.vmid();
                let guest_type = entry.guest_type();
                let link = ctx.link().clone();
                ctx.link().spawn(async move {
                    let client = crate::pdm_client();
                    let res = match (guest_type, action) {
                        (GuestType::Qemu, Action::Start) => {
                            client.pve_qemu_start(&remote, Some(&node), vmid).await
                        }
                        (GuestType::Qemu, Action::Shutdown) => {
                            client.pve_qemu_shutdown(&remote, Some(&node), vmid).await
                        }
                        (GuestType::Lxc, Action::Start) => {
                            client.pve_lxc_start(&remote, Some(&node), vmid).await
                        }
                        (GuestType::Lxc, Action::Shutdown) => {
                            client.pve_lxc_shutdown(&remote, Some(&node), vmid).await
                        }
                    };
                    match res {
                        Ok(upid) => link.send_message(Msg::ShowTask(upid)),
                        Err(err) => link.show_error(tr!("Error"), err.to_string(), true),
                    }
                });
            }
        }
        true
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        let link = ctx.link();
        let total = self.store.data_len();
        let shown = self.store.filtered_data_len();
        let count_text = if shown == total {
            tr!("{n} Guest" | "{n} Guests" % total)
        } else {
            tr!(
                "{0} out of {1} Guest" | "{0} out of {1} Guests" % total,
                shown,
                total
            )
        };
        // reserve room for the widest (plural) count string at the total's
        // magnitude so typing in the filter doesn't reflow the toolbar
        let count_reserve = tr!(
            "{0} out of {1} Guest" | "{0} out of {1} Guests" % 2,
            total,
            total
        )
        .chars()
        .count()
            + 2;
        let mode = *self.view_mode;
        let flat_active = mode == ViewMode::Flat;
        let tree_active = mode == ViewMode::Tree;
        let view_toggle = SegmentedButton::new()
            .aria_label(tr!("View mode"))
            .with_button(
                Button::new(tr!("List"))
                    .icon_class("fa fa-list-ul")
                    .class(flat_active.then_some(ColorScheme::Primary))
                    .pressed(flat_active)
                    .attribute("aria-pressed", if flat_active { "true" } else { "false" })
                    .on_activate(link.callback(|_| Msg::SetViewMode(ViewMode::Flat))),
            )
            .with_button(
                Button::new(tr!("Tree"))
                    .icon_class("fa fa-sitemap")
                    .class(tree_active.then_some(ColorScheme::Primary))
                    .pressed(tree_active)
                    .attribute("aria-pressed", if tree_active { "true" } else { "false" })
                    .on_activate(link.callback(|_| Msg::SetViewMode(ViewMode::Tree))),
            );

        Some(
            Toolbar::new()
                .border_bottom(true)
                .with_child(
                    Field::new()
                        .value(self.filter.clone())
                        .attribute("aria-label", AttrValue::from(tr!("Filter guests")))
                        .with_trigger(
                            Trigger::new(if self.filter.is_empty() {
                                ""
                            } else {
                                "fa fa-times"
                            })
                            .tip(tr!("Clear filter"))
                            .attribute("aria-label", AttrValue::from(tr!("Clear filter")))
                            .on_activate(link.callback(|_| Msg::Filter(String::new()))),
                            true,
                        )
                        .placeholder(tr!("Filter"))
                        .on_input(link.callback(Msg::Filter)),
                )
                .with_child(
                    Container::new()
                        .style("min-width", format!("{count_reserve}ch"))
                        .with_child(count_text),
                )
                .with_flex_spacer()
                .with_child(view_toggle)
                .with_child(Button::refresh(self.loading()).on_activate({
                    let link = link.clone();
                    move |_| link.send_reload()
                }))
                .into(),
        )
    }

    fn main_view(&self, _ctx: &LoadableComponentContext<Self>) -> Html {
        let total = self.store.data_len();
        let visible = self.store.filtered_data_len();

        let mut column = Column::new().class(FlexFit);
        if !self.failed_remotes.is_empty() {
            column.add_child(failed_remotes_banner(&self.failed_remotes));
        }

        if self.loading() && total == 0 {
            // initial load in flight: show a centered spinner instead of a
            // misleading "no guests" message
            column.add_child(
                Column::new()
                    .class(FlexFit)
                    .class(JustifyContent::Center)
                    .class(AlignItems::Center)
                    .with_child(Container::from_tag("i").class("pwt-loading-icon")),
            );
        } else if visible == 0 {
            // DataTable has no placeholder in this toolkit version, so render an
            // explicit, centered empty state that tells the three cases apart.
            let state = if self.remote_count == 0 {
                empty_state(
                    "server",
                    tr!("No remotes configured yet"),
                    tr!("Add a Proxmox VE remote on the Configuration tab to see its guests here."),
                )
            } else if total == 0 {
                empty_state(
                    "desktop",
                    tr!("No guests found"),
                    tr!("None of the connected remotes have any virtual machines or containers."),
                )
            } else {
                empty_state(
                    "search",
                    tr!("No matching guests"),
                    tr!("No guest matches the current filter."),
                )
            };
            column.add_child(state);
        } else {
            let table: Html = match *self.view_mode {
                ViewMode::Flat => DataTable::new(self.flat_columns.clone(), self.store.clone())
                    .selection(self.selection.clone())
                    .striped(true)
                    .hover(true)
                    .class(FlexFit)
                    .into(),
                ViewMode::Tree => {
                    DataTable::new(self.tree_columns.clone(), self.tree_store.clone())
                        .selection(self.selection.clone())
                        .hover(true)
                        .class(FlexFit)
                        .into()
                }
            };
            column.add_child(table);
        }
        column.into()
    }

    fn dialog_view(
        &self,
        ctx: &LoadableComponentContext<Self>,
        view_state: &Self::ViewState,
    ) -> Option<Html> {
        match view_state {
            ViewState::Confirm(action, key) => {
                let label = self
                    .store
                    .read()
                    .lookup_record(key)
                    .map(|entry| render_resource_name(&entry.resource, false))
                    .unwrap_or_else(|| key.to_string());
                // full sentences per action so translators never see concatenated fragments
                let message = match action {
                    Action::Start => tr!("Are you sure you want to start guest '{0}'?", label),
                    Action::Shutdown => {
                        tr!("Are you sure you want to shut down guest '{0}'?", label)
                    }
                };
                let action = action.clone();
                let key = key.clone();
                Some(
                    MessageBox::new(tr!("Confirm"), message)
                        .buttons(MessageBoxButtons::YesNo)
                        .on_close({
                            let link = ctx.link().clone();
                            move |confirm| {
                                if confirm {
                                    link.send_message(Msg::GuestAction(
                                        action.clone(),
                                        key.clone(),
                                    ));
                                }
                                link.change_view(None);
                            }
                        })
                        .into(),
                )
            }
            ViewState::Migrate(remote, guest_info) => Some(
                MigrateWindow::new(remote.clone(), *guest_info)
                    .on_close(ctx.link().change_view_callback(|_| None))
                    .on_submit({
                        let link = ctx.link().clone();
                        move |upid: RemoteUpid| link.send_message(Msg::ShowTask(upid))
                    })
                    .into(),
            ),
        }
    }

    fn load(
        &self,
        ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        let link = ctx.link().clone();
        Box::pin(async move {
            // Fetch all resources and filter to guests client-side (below). We
            // deliberately avoid a `search` filter like "type:qemu type:lxc": the
            // server drops remotes with no matching resources, which would also
            // drop failed/unreachable remotes and silently break the
            // failed-remotes banner. A future API taking a typed list of
            // resource-types would let us narrow to guests server-side (the
            // search is left empty, so failed remotes are still returned).
            // `None` lets the server apply its default cache max-age.
            let remotes = crate::pdm_client().resources(None, None).await?;
            link.send_message(Msg::LoadFinished(remotes));
            Ok(())
        })
    }
}

/// A centered, large-icon empty state with a title and an explanatory hint.
fn empty_state(icon: &str, title: String, hint: String) -> Html {
    Column::new()
        .class(FlexFit)
        .class(JustifyContent::Center)
        .class(AlignItems::Center)
        .gap(2)
        .padding(4)
        .with_child(Fa::new(icon).large_3x().class(FontColor::NeutralAlt))
        .with_child(
            Container::from_tag("span")
                .class(FontStyle::TitleMedium)
                .with_child(title),
        )
        .with_child(
            Container::from_tag("span")
                .class(FontColor::NeutralAlt)
                .with_child(hint),
        )
        .into()
}

fn failed_remotes_banner(failed: &[String]) -> Html {
    Row::new()
        .padding(2)
        .gap(2)
        .class(AlignItems::Center)
        // status live region for the unreachable-remotes warning
        .attribute("role", "status")
        .with_child(Fa::new("exclamation-triangle").class(ColorScheme::Warning))
        .with_child(tr!("Could not query some remotes: {0}", failed.join(", ")))
        .into()
}

/// Build the Remote->Guest tree. `expand` should only be true on the first build;
/// on rebuilds the remote nodes are left at their default so `update_root_tree`
/// can restore the user's manual expand/collapse state (forcing them open every
/// reload would otherwise undo a collapse).
fn build_guest_tree(entries: &[GuestEntry], expand: bool) -> KeyedSlabTree<GuestTreeNode> {
    let mut by_remote: BTreeMap<String, Vec<GuestEntry>> = BTreeMap::new();
    for entry in entries {
        by_remote
            .entry(entry.remote.clone())
            .or_default()
            .push(entry.clone());
    }

    let mut tree = KeyedSlabTree::new();
    let mut root = tree.set_root(GuestTreeNode::Root);
    for (remote, mut guests) in by_remote {
        guests.sort_by_key(|guest| guest.vmid());
        let mut remote_node = root.append(GuestTreeNode::Remote(remote, guests.len()));
        if expand {
            remote_node.set_expanded(true);
        }
        for guest in guests {
            remote_node.append(GuestTreeNode::Guest(guest));
        }
    }
    // the synthetic root is hidden (view_root(false)) but must stay expanded for
    // its remote children to render
    root.set_expanded(true);
    tree
}

fn guest_matches(entry: &GuestEntry, text: &str) -> bool {
    entry.remote.to_lowercase().contains(text)
        || entry.resource.name().to_lowercase().contains(text)
        || entry.vmid().to_string().contains(text)
        || entry.resource.status().to_lowercase().contains(text)
        || entry.guest_type().to_string().contains(text)
        || entry.node().to_lowercase().contains(text)
        || entry
            .tags()
            .iter()
            .any(|tag| tag.to_lowercase().contains(text))
}

// --- shared cell renderers, used by both the flat and the tree columns ---

fn guest_label(entry: &GuestEntry) -> Html {
    render_tree_column(
        render_status_icon(&entry.resource).into(),
        entry.resource.name().to_string(),
    )
    .into()
}

fn status_html(entry: &GuestEntry) -> Html {
    guest_status_label(entry.resource.status()).into()
}

fn cpu_html(entry: &GuestEntry) -> Html {
    rrd_value_renderer::render_cpu_usage(&entry.cpu()).into()
}

fn mem_html(entry: &GuestEntry) -> Html {
    tr!(
        "{0} of {1}",
        HumanByte::from(entry.mem()),
        HumanByte::from(entry.maxmem())
    )
    .into()
}

fn uptime_html(entry: &GuestEntry) -> Html {
    let uptime = entry.uptime();
    if uptime == 0 {
        String::from("-").into()
    } else {
        format_duration_human(uptime as f64).into()
    }
}

fn guest_actions(link: &LoadableComponentScope<GuestPanelComp>, entry: &GuestEntry) -> Html {
    let key = entry.key();
    let status = entry.resource.status().to_string();
    let template = entry.template();
    let remote = entry.remote.clone();
    let node = entry.node().to_string();
    let local_id = entry.resource.id();
    let guest_info = entry.guest_info();

    Row::new()
        .gap(1)
        .class(JustifyContent::FlexEnd)
        .with_optional_child((!template).then(|| {
            // a paused guest is still live and can be shut down
            let disabled = !guest_is_live(&status);
            Tooltip::new(
                ActionIcon::new("fa fa-fw fa-power-off")
                    .disabled(disabled)
                    .class((!disabled).then_some(ColorScheme::Error))
                    .attribute("aria-label", AttrValue::from(tr!("Shutdown")))
                    .on_activate({
                        let link = link.clone();
                        let key = key.clone();
                        move |_| {
                            link.change_view(Some(ViewState::Confirm(
                                Action::Shutdown,
                                key.clone(),
                            )))
                        }
                    }),
            )
            .tip(tr!("Shutdown"))
        }))
        .with_optional_child((!template).then(|| {
            // only a non-live (stopped) guest can be started
            let disabled = guest_is_live(&status);
            Tooltip::new(
                ActionIcon::new("fa fa-fw fa-play")
                    .disabled(disabled)
                    .class((!disabled).then_some(ColorScheme::Success))
                    .attribute("aria-label", AttrValue::from(tr!("Start")))
                    .on_activate({
                        let link = link.clone();
                        let key = key.clone();
                        move |_| {
                            link.change_view(Some(ViewState::Confirm(Action::Start, key.clone())))
                        }
                    }),
            )
            .tip(tr!("Start"))
        }))
        .with_optional_child((!template).then(|| {
            let remote = remote.clone();
            Tooltip::new(
                ActionIcon::new("fa fa-fw fa-paper-plane-o")
                    .attribute("aria-label", AttrValue::from(tr!("Migrate")))
                    .on_activate({
                        let link = link.clone();
                        move |_| {
                            link.change_view(Some(ViewState::Migrate(remote.clone(), guest_info)))
                        }
                    }),
            )
            .tip(tr!("Migrate"))
        }))
        .with_child(
            Tooltip::new(
                ActionIcon::new("fa fa-fw fa-external-link")
                    .attribute("aria-label", AttrValue::from(tr!("Open in PVE UI")))
                    .on_activate({
                        let link = link.clone();
                        move |_| {
                            if let Some(url) = get_deep_url(&link, &remote, Some(&node), &local_id)
                            {
                                let _ = window().open_with_url(&url.href());
                            }
                        }
                    }),
            )
            .tip(tr!("Open in PVE UI")),
        )
        .into()
}

fn flat_columns(
    link: LoadableComponentScope<GuestPanelComp>,
) -> Rc<Vec<DataTableHeader<GuestEntry>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Name"))
            .flex(2)
            .render(|entry: &GuestEntry| guest_label(entry))
            .sorter(|a: &GuestEntry, b: &GuestEntry| a.resource.name().cmp(b.resource.name()))
            .into(),
        DataTableColumn::new(tr!("ID"))
            .width("80px")
            .get_property_owned(|entry: &GuestEntry| entry.vmid())
            .into(),
        DataTableColumn::new(tr!("Status"))
            .width("110px")
            .render(|entry: &GuestEntry| status_html(entry))
            .sorter(|a: &GuestEntry, b: &GuestEntry| a.resource.status().cmp(b.resource.status()))
            .into(),
        DataTableColumn::new(tr!("Remote"))
            .flex(1)
            .get_property(|entry: &GuestEntry| entry.remote.as_str())
            // override the get_property sorter to group by remote, then VMID
            .sorter(|a: &GuestEntry, b: &GuestEntry| {
                a.remote.cmp(&b.remote).then(a.vmid().cmp(&b.vmid()))
            })
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Node"))
            .flex(1)
            .get_property(|entry: &GuestEntry| entry.node())
            .into(),
        DataTableColumn::new(tr!("Tags"))
            .flex(1)
            .render(|entry: &GuestEntry| render_guest_tags(entry.tags()).into())
            .into(),
        DataTableColumn::new(tr!("CPU usage"))
            .width("90px")
            .sorter(|a: &GuestEntry, b: &GuestEntry| {
                a.cpu()
                    .partial_cmp(&b.cpu())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .render(|entry: &GuestEntry| cpu_html(entry))
            .into(),
        DataTableColumn::new(tr!("Memory usage"))
            .width("150px")
            .sorter(|a: &GuestEntry, b: &GuestEntry| a.mem().cmp(&b.mem()))
            .render(|entry: &GuestEntry| mem_html(entry))
            .into(),
        DataTableColumn::new(tr!("Uptime"))
            .width("100px")
            .sorter(|a: &GuestEntry, b: &GuestEntry| a.uptime().cmp(&b.uptime()))
            .render(|entry: &GuestEntry| uptime_html(entry))
            .into(),
        DataTableColumn::new(tr!("Actions"))
            .width("180px")
            .render(move |entry: &GuestEntry| guest_actions(&link, entry))
            .into(),
    ])
}

fn tree_columns(
    link: LoadableComponentScope<GuestPanelComp>,
    store: TreeStore<GuestTreeNode>,
) -> Rc<Vec<DataTableHeader<GuestTreeNode>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Name"))
            .flex(2)
            .tree_column(store)
            .render(|node: &GuestTreeNode| match node {
                GuestTreeNode::Guest(entry) => guest_label(entry),
                GuestTreeNode::Remote(name, count) => {
                    render_tree_column(Fa::new("server").into(), format!("{name} ({count})")).into()
                }
                GuestTreeNode::Root => html! {},
            })
            .into(),
        DataTableColumn::new(tr!("ID"))
            .width("80px")
            .render(|node: &GuestTreeNode| match node {
                GuestTreeNode::Guest(entry) => html! { {entry.vmid()} },
                _ => html! {},
            })
            .into(),
        DataTableColumn::new(tr!("Status"))
            .width("110px")
            .render(|node: &GuestTreeNode| match node {
                GuestTreeNode::Guest(entry) => status_html(entry),
                _ => html! {},
            })
            .into(),
        DataTableColumn::new(tr!("Node"))
            .flex(1)
            .render(|node: &GuestTreeNode| match node {
                GuestTreeNode::Guest(entry) => html! { {entry.node()} },
                _ => html! {},
            })
            .into(),
        DataTableColumn::new(tr!("Tags"))
            .flex(1)
            .render(|node: &GuestTreeNode| match node {
                GuestTreeNode::Guest(entry) => render_guest_tags(entry.tags()).into(),
                _ => html! {},
            })
            .into(),
        DataTableColumn::new(tr!("CPU usage"))
            .width("90px")
            .render(|node: &GuestTreeNode| match node {
                GuestTreeNode::Guest(entry) => cpu_html(entry),
                _ => html! {},
            })
            .into(),
        DataTableColumn::new(tr!("Memory usage"))
            .width("150px")
            .render(|node: &GuestTreeNode| match node {
                GuestTreeNode::Guest(entry) => mem_html(entry),
                _ => html! {},
            })
            .into(),
        DataTableColumn::new(tr!("Uptime"))
            .width("100px")
            .render(|node: &GuestTreeNode| match node {
                GuestTreeNode::Guest(entry) => uptime_html(entry),
                _ => html! {},
            })
            .into(),
        DataTableColumn::new(tr!("Actions"))
            .width("180px")
            .render(move |node: &GuestTreeNode| match node {
                GuestTreeNode::Guest(entry) => guest_actions(&link, entry),
                _ => html! {},
            })
            .into(),
    ])
}
