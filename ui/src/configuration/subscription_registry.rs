use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;

use yew::virtual_dom::{Key, VComp, VNode};

use proxmox_yew_comp::percent_encoding::percent_encode_component;
use proxmox_yew_comp::utils::render_epoch;
use proxmox_yew_comp::{http_delete, http_get, http_get_full, http_post};
use proxmox_yew_comp::{
    LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState,
};

use pwt::css::{
    AlignItems, ColorScheme, Display, Flex, FlexDirection, FlexFit, FontColor, JustifyContent,
    Overflow,
};
use pwt::prelude::*;
use pwt::props::{ContainerBuilder, ExtractPrimaryKey, WidgetBuilder};
use pwt::state::{Selection, SlabTree, Store, TreeStore};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader, MultiSelectMode};
use pwt::widget::form::{Combobox, Field};
use pwt::widget::menu::{Menu, MenuButton, MenuItem};
use pwt::widget::{
    Button, Column, Container, Fa, Mask, MiniScrollMode, Panel, Row, SegmentedButton, Toolbar,
    Tooltip, Trigger,
};

use pdm_api_types::subscription::{
    socket_count_from_key, AutoAssignProposal, ProposedAssignment, RemoteNodeStatus,
    SubscriptionKeyEntry, SubscriptionLevel,
};

use super::subscription_assign::{AssignKeyToNodeDialog, AssignTarget};
use super::subscription_keys::{empty_state_hint, SubscriptionKeyGrid};

const NODE_STATUS_URL: &str = "/subscriptions/node-status";
const KEYS_URL: &str = "/subscriptions/keys";
const AUTO_ASSIGN_URL: &str = "/subscriptions/auto-assign";
const BULK_ASSIGN_URL: &str = "/subscriptions/bulk-assign";
const APPLY_PENDING_URL: &str = "/subscriptions/apply-pending";
const CLEAR_PENDING_URL: &str = "/subscriptions/clear-pending";

/// Map a [`SubscriptionStatus`] to the icon shown in subscription panels.
///
/// Public so the dashboard subscriptions panel can render the same icon for the same state
/// without redefining the mapping. The 4-variant `proxmox_yew_comp::Status` does not cover
/// every subscription state (New, Expired, Suspended need their own icons), hence the dedicated
/// helper.
pub fn subscription_status_icon(status: proxmox_subscription::SubscriptionStatus) -> Fa {
    use proxmox_subscription::SubscriptionStatus as S;
    match status {
        S::Active => Fa::new("check-circle").class(FontColor::Success),
        S::New => Fa::new("clock-o").class(FontColor::Primary),
        S::NotFound => Fa::new("exclamation-circle").class(FontColor::Error),
        S::Invalid => Fa::new("times-circle").class(FontColor::Warning),
        S::Expired => Fa::new("clock-o").class(FontColor::Warning),
        S::Suspended => Fa::new("ban").class(FontColor::Error),
    }
}

fn subscription_status_label(status: proxmox_subscription::SubscriptionStatus) -> String {
    use proxmox_subscription::SubscriptionStatus as S;
    match status {
        S::Active => tr!("Active"),
        S::New => tr!("New"),
        S::NotFound => tr!("No subscription"),
        S::Invalid => tr!("Invalid"),
        S::Expired => tr!("Expired"),
        S::Suspended => tr!("Suspended"),
    }
}

/// Every subscription status, in the order they appear in the node-status filter.
fn all_node_statuses() -> [proxmox_subscription::SubscriptionStatus; 6] {
    use proxmox_subscription::SubscriptionStatus as S;
    [
        S::Active,
        S::New,
        S::NotFound,
        S::Invalid,
        S::Expired,
        S::Suspended,
    ]
}

/// True when a node passes the active filters: its status is selected (or no status is selected,
/// meaning all), and the already-lowercased `text` is empty or a substring of its node or remote
/// name.
fn node_matches(
    n: &RemoteNodeStatus,
    text: &str,
    statuses: &[proxmox_subscription::SubscriptionStatus],
) -> bool {
    let status_ok = statuses.is_empty() || statuses.contains(&n.status);
    let text_ok = text.is_empty()
        || n.node.to_lowercase().contains(text)
        || n.remote.to_lowercase().contains(text);
    status_ok && text_ok
}

/// Build a multi-line Status-column tooltip listing the last-check timestamp and the
/// next-due-date when the remote provides them. Returns None if neither is set so the caller
/// can skip wrapping the cell in a tooltip entirely.
fn status_tooltip_lines(n: &RemoteNodeStatus) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    if let Some(ts) = n.check_time {
        lines.push(tr!("Last checked: {when}", when = render_epoch(ts)));
    }
    if let Some(due) = n.next_due_date.as_deref() {
        if !due.is_empty() {
            lines.push(tr!("Next due: {date}", date = due.to_string()));
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

fn pending_badge(push_count: u32, clear_count: u32) -> Row {
    let mut row = Row::new().class(AlignItems::Center).gap(3);
    if push_count > 0 {
        row = row.with_child(
            Tooltip::new(
                Row::new()
                    .class(AlignItems::Baseline)
                    .gap(1)
                    .with_child(Fa::new("clock-o").class(FontColor::Warning))
                    .with_child(tr!("{n} pending push(es)", n = push_count)),
            )
            .tip(tr!(
                "{n} pool key(s) queued for push; Apply Pending will install them on the remote.",
                n = push_count,
            )),
        );
    }
    if clear_count > 0 {
        row = row.with_child(
            Tooltip::new(
                Row::new()
                    .class(AlignItems::Baseline)
                    .gap(1)
                    .with_child(Fa::new("recycle").class(FontColor::Warning))
                    .with_child(tr!("{n} pending clear(s)", n = clear_count)),
            )
            .tip(tr!(
                "{n} live subscription(s) queued for removal; Apply Pending will free them.",
                n = clear_count,
            )),
        );
    }
    row
}

/// Row shape for the Adopt All preview table.
#[derive(Clone, PartialEq)]
struct AdoptCandidate {
    remote: String,
    node: String,
    key: String,
}

/// Per-row picker state for one Auto-Assign proposal row: the keys the operator may choose from
/// (its own current key plus every free pool key not claimed by another row, so no two rows can
/// pick the same key) and the socket count of the currently chosen key.
struct ProposalRowChoice {
    options: Rc<Vec<yew::AttrValue>>,
    current: String,
    key_sockets: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
enum NodeTreeEntry {
    Root,
    Remote {
        name: String,
        ty: pdm_api_types::remotes::RemoteType,
        active: u32,
        total: u32,
    },
    Node {
        data: RemoteNodeStatus,
        /// If true, this is the only node in its remote and is shown at the top level under the
        /// remote name instead of nested.
        standalone: bool,
    },
}

impl NodeTreeEntry {
    fn name(&self) -> &str {
        match self {
            Self::Root => "",
            Self::Remote { name, .. } => name,
            Self::Node { data, standalone } => {
                if *standalone {
                    &data.remote
                } else {
                    &data.node
                }
            }
        }
    }
}

impl ExtractPrimaryKey for NodeTreeEntry {
    fn extract_key(&self) -> Key {
        Key::from(match self {
            NodeTreeEntry::Root => "/".to_string(),
            NodeTreeEntry::Remote { name, .. } => format!("/{name}"),
            NodeTreeEntry::Node { data, .. } => format!("/{}/{}", data.remote, data.node),
        })
    }
}

fn build_tree(nodes: Vec<RemoteNodeStatus>) -> SlabTree<NodeTreeEntry> {
    use std::collections::BTreeMap;

    let mut by_remote: BTreeMap<String, Vec<RemoteNodeStatus>> = BTreeMap::new();
    for n in nodes {
        by_remote.entry(n.remote.clone()).or_default().push(n);
    }

    let mut tree = SlabTree::new();
    let mut root = tree.set_root(NodeTreeEntry::Root);
    root.set_expanded(true);

    for (remote_name, mut remote_nodes) in by_remote {
        let total = remote_nodes.len() as u32;
        let active = remote_nodes
            .iter()
            .filter(|n| n.status == proxmox_subscription::SubscriptionStatus::Active)
            .count() as u32;

        let ty = remote_nodes.first().map(|n| n.ty).unwrap_or_default();

        if remote_nodes.len() == 1 {
            root.append(NodeTreeEntry::Node {
                data: remote_nodes.remove(0),
                standalone: true,
            });
        } else {
            let mut remote_entry = root.append(NodeTreeEntry::Remote {
                name: remote_name,
                ty,
                active,
                total,
            });
            remote_entry.set_expanded(true);
            for n in remote_nodes {
                remote_entry.append(NodeTreeEntry::Node {
                    data: n,
                    standalone: false,
                });
            }
        }
    }

    tree
}

#[derive(Properties, PartialEq, Clone)]
pub struct SubscriptionRegistryProps {}

impl SubscriptionRegistryProps {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

impl Default for SubscriptionRegistryProps {
    fn default() -> Self {
        Self::new()
    }
}

impl From<SubscriptionRegistryProps> for VNode {
    fn from(val: SubscriptionRegistryProps) -> Self {
        VComp::new::<LoadableComponentMaster<SubscriptionRegistryComp>>(Rc::new(val), None).into()
    }
}

/// What the per-node Revert button should do on the selected entry.
enum RevertTarget {
    /// Drop the pool's binding (the key was assigned but never pushed). Carries the pool key.
    Unassign(String),
    /// Cancel a queued Clear Key while keeping the binding intact.
    CancelClear { remote: String, node: String },
}

pub enum Msg {
    LoadFinished {
        nodes: Vec<RemoteNodeStatus>,
        keys: Vec<SubscriptionKeyEntry>,
        digest: Option<String>,
    },
    AutoAssignPreview,
    /// Open the proposal dialog with every row pre-selected.
    ShowProposal(AutoAssignProposal),
    /// Pin a different free pool key to one proposal row. Carries the row key "{remote}/{node}",
    /// the chosen key (empty when the picker was cleared), and the row's auto-picked key.
    OverrideProposalKey(Key, String, String),
    /// Commit a previously-fetched proposal via the bulk-assign endpoint.
    BulkAssignApply(AutoAssignProposal),
    ApplyPending,
    ClearPending,
    /// Revert the pending change on the currently-selected node: drop an unpushed binding or
    /// cancel a queued Clear Key (dispatched on the [`RevertTarget`] variant).
    RevertSelectedNode,
    /// Open the confirmation dialog for queueing a clear on the selected node.
    QueueClearForSelectedNode,
    /// Open the Assign Key dialog for the currently-selected node.
    AssignKeyToSelectedNode,
    /// Open the confirmation dialog for adopting the live subscription on the selected node
    /// into the pool.
    AdoptKeyForSelectedNode,
    /// Open the confirmation dialog for adopting every foreign live subscription into the pool.
    AdoptAllPreview,
    /// Re-check the subscription on the currently-selected node against the shop. Pure refresh
    /// path; no confirmation dialog since the action is read-only from the pool's perspective.
    CheckSubscriptionForSelectedNode,
    /// Show or hide the collapsible filter section.
    ToggleFilterPanel,
    /// Set the free-text remote/node filter.
    SetNodeFilterText(String),
    /// Toggle one status in the multi-select status filter.
    ToggleStatusFilter(proxmox_subscription::SubscriptionStatus),
    /// Clear the status filter (the "All" segment).
    ClearStatusFilter,
}

#[derive(PartialEq)]
pub enum ViewState {
    /// Auto-Assign proposal is being computed on the server (fans out to every remote, so it can
    /// take seconds on a slow fleet). Shown immediately on click so the operator sees the request
    /// landed instead of staring at an unchanged panel.
    AutoAssignComputing,
    ConfirmAutoAssign(AutoAssignProposal),
    ConfirmApplyPending,
    ConfirmClearPending,
    /// Pending confirmation to queue a clear for `(remote, node)`. The current key on the
    /// node is shown in the dialog body when available.
    ConfirmQueueClear {
        remote: String,
        node: String,
        current_key: Option<String>,
    },
    /// Pending confirmation to adopt the live subscription on `(remote, node)` into the pool.
    /// The live key is captured here so the dialog body can show what will be imported.
    ConfirmAdoptKey {
        remote: String,
        node: String,
        current_key: String,
    },
    /// Pending confirmation to bulk-adopt every foreign live subscription. The candidate list
    /// is captured at view-open time so the dialog body can show the operator exactly what
    /// will be imported; the server re-computes the set under the lock at commit time.
    ConfirmAdoptAll {
        candidates: Vec<(String, String, String)>,
    },
    /// Assign a pool key to the given node. Opens from the right panel's Assign Key button.
    AssignKeyToNode(AssignTarget),
}

#[doc(hidden)]
pub struct SubscriptionRegistryComp {
    state: LoadableComponentState<ViewState>,
    tree_store: TreeStore<NodeTreeEntry>,
    tree_columns: Rc<Vec<DataTableHeader<NodeTreeEntry>>>,
    /// Multi-select state for the Auto-Assign proposal: every row starts ticked, and the operator
    /// can untick a few before applying. Held here so toggling survives the dialog re-render.
    proposal_selection: Selection,
    /// Per-row key overrides for the Auto-Assign proposal, keyed by "{remote}/{node}". Empty means
    /// every row keeps its auto-picked key. Cleared whenever a fresh proposal is shown.
    proposal_overrides: HashMap<Key, String>,
    /// Per-row counter bumped each time a proposal row's key picker is cleared. Folded into the
    /// picker's vdom key so a clear rebuilds the cell from the effective key instead of leaving
    /// the controlled field stuck on the empty value it cannot resync from.
    proposal_clear_nonce: HashMap<Key, u32>,
    adopt_columns: Rc<Vec<DataTableHeader<AdoptCandidate>>>,
    node_selection: Selection,
    last_node_data: Vec<RemoteNodeStatus>,
    /// Canonical pool snapshot. Passed down to the key grid (display) and shared with the
    /// node-first Assign dialog and the Add+Assign wizard (selector source-of-truth).
    pool_keys: Rc<Vec<SubscriptionKeyEntry>>,
    /// Pool-config digest captured alongside `pool_keys`. Forwarded to every pool mutation so
    /// the server rejects stale-view writes with 409 instead of silently overwriting a parallel
    /// admin's edits.
    pool_digest: Option<String>,
    /// Whether the collapsible filter section is shown.
    filter_expanded: bool,
    /// Free-text remote/node filter.
    node_text_filter: String,
    /// Active node-tree status filter; empty means every status passes.
    node_status_filter: Vec<proxmox_subscription::SubscriptionStatus>,
}

pwt::impl_deref_mut_property!(
    SubscriptionRegistryComp,
    state,
    LoadableComponentState<ViewState>
);

fn tree_sorter(a: &NodeTreeEntry, b: &NodeTreeEntry) -> std::cmp::Ordering {
    a.name().cmp(b.name())
}

/// Sort helper that compares two Node entries on a derived key and falls back to name comparison
/// for any Root/Remote variant; tree columns surface this so parent rows do not reshuffle when
/// sorting by a Node-only attribute.
fn node_field_sorter<K: Ord>(
    a: &NodeTreeEntry,
    b: &NodeTreeEntry,
    f: impl Fn(&RemoteNodeStatus) -> K,
) -> std::cmp::Ordering {
    match (a, b) {
        (NodeTreeEntry::Node { data: na, .. }, NodeTreeEntry::Node { data: nb, .. }) => {
            f(na).cmp(&f(nb))
        }
        _ => a.name().cmp(b.name()),
    }
}

impl SubscriptionRegistryComp {
    fn tree_columns(store: TreeStore<NodeTreeEntry>) -> Rc<Vec<DataTableHeader<NodeTreeEntry>>> {
        Rc::new(vec![
            DataTableColumn::new(tr!("Name"))
                .tree_column(store)
                .flex(3)
                .render(|entry: &NodeTreeEntry| {
                    let (icon, name) = match entry {
                        NodeTreeEntry::Root => return Html::default(),
                        NodeTreeEntry::Remote { name, ty, .. } => {
                            let icon = if *ty == pdm_api_types::remotes::RemoteType::Pbs {
                                "building-o"
                            } else {
                                "server"
                            };
                            (icon, name.as_str())
                        }
                        NodeTreeEntry::Node {
                            data: n,
                            standalone,
                        } => {
                            let icon = if n.ty == pdm_api_types::remotes::RemoteType::Pbs {
                                "building-o"
                            } else {
                                "building"
                            };
                            let label = if *standalone { &n.remote } else { &n.node };
                            (icon, label.as_str())
                        }
                    };
                    Row::new()
                        .class(AlignItems::Baseline)
                        .gap(2)
                        .with_child(Fa::new(icon))
                        .with_child(name)
                        .into()
                })
                .sorter(tree_sorter)
                .into(),
            DataTableColumn::new(tr!("Sockets"))
                .width("80px")
                .sorter(|a: &NodeTreeEntry, b: &NodeTreeEntry| {
                    node_field_sorter(a, b, |n| n.sockets)
                })
                .render(|entry: &NodeTreeEntry| match entry {
                    NodeTreeEntry::Node { data: n, .. } => {
                        n.sockets.map(|s| s.to_string()).unwrap_or_default().into()
                    }
                    _ => Html::default(),
                })
                .into(),
            DataTableColumn::new(tr!("Status"))
                .width("150px")
                .sorter(|a: &NodeTreeEntry, b: &NodeTreeEntry| {
                    node_field_sorter(a, b, |n| subscription_status_label(n.status))
                })
                .render(|entry: &NodeTreeEntry| match entry {
                    NodeTreeEntry::Node { data: n, .. } => {
                        let row = Row::new()
                            .class(AlignItems::Baseline)
                            .gap(2)
                            .with_child(subscription_status_icon(n.status))
                            .with_child(subscription_status_label(n.status));
                        status_tooltip_lines(n)
                            .map(|tip| Tooltip::new(row.clone()).tip(tip).into())
                            .unwrap_or_else(|| row.into())
                    }
                    NodeTreeEntry::Remote { active, total, .. } => {
                        let icon = if active == total {
                            Fa::new("check-circle").class(FontColor::Success)
                        } else if *active == 0 {
                            Fa::new("exclamation-circle").class(FontColor::Error)
                        } else {
                            Fa::new("exclamation-triangle").class(FontColor::Warning)
                        };
                        Tooltip::new(
                            Row::new()
                                .class(AlignItems::Baseline)
                                .gap(2)
                                .with_child(icon)
                                .with_child(format!("{active}/{total}")),
                        )
                        .tip(tr!(
                            "{active} of {total} nodes subscribed",
                            active = active,
                            total = total,
                        ))
                        .into()
                    }
                    _ => Html::default(),
                })
                .into(),
            DataTableColumn::new(tr!("Level"))
                .width("100px")
                .sorter(|a: &NodeTreeEntry, b: &NodeTreeEntry| node_field_sorter(a, b, |n| n.level))
                .render(|entry: &NodeTreeEntry| match entry {
                    NodeTreeEntry::Node { data: n, .. } if n.level != SubscriptionLevel::None => {
                        n.level.to_string().into()
                    }
                    _ => Html::default(),
                })
                .into(),
            DataTableColumn::new(tr!("Key"))
                .flex(2)
                .sorter(|a: &NodeTreeEntry, b: &NodeTreeEntry| {
                    node_field_sorter(a, b, |n| {
                        n.assigned_key
                            .clone()
                            .or_else(|| n.current_key.clone())
                            .unwrap_or_default()
                    })
                })
                .render(|entry: &NodeTreeEntry| match entry {
                    NodeTreeEntry::Node { data: n, .. } => key_cell(n),
                    _ => Html::default(),
                })
                .into(),
        ])
    }

    /// Build the Auto-Assign proposal columns for the current proposal and overrides. The leading
    /// checkbox column drives the per-row opt-out, and the Key column is a dropdown for pinning a
    /// specific free pool key; both depend on live state, so the columns are rebuilt per render.
    /// Menu and resize are independent flags, disable both on the data columns.
    fn proposal_columns(
        &self,
        ctx: &LoadableComponentContext<Self>,
        proposal: &AutoAssignProposal,
    ) -> Rc<Vec<DataTableHeader<ProposedAssignment>>> {
        // Free pool keys split by product. A row may pick any free key of its product that no
        // other row currently holds, plus its own current key, so two rows can never claim the
        // same key and the result needs no conflict check.
        let mut free_pve: Vec<String> = Vec::new();
        let mut free_pbs: Vec<String> = Vec::new();
        for e in self.pool_keys.iter() {
            if e.remote.is_some() {
                continue;
            }
            if e.key.starts_with("pbs") {
                free_pbs.push(e.key.clone());
            } else if e.key.starts_with("pve") {
                free_pve.push(e.key.clone());
            }
        }
        free_pve.sort();
        free_pbs.sort();

        let effective = |p: &ProposedAssignment| -> String {
            let rk = Key::from(format!("{}/{}", p.remote, p.node));
            self.proposal_overrides
                .get(&rk)
                .cloned()
                .unwrap_or_else(|| p.key.clone())
        };
        let taken: HashSet<String> = proposal.assignments.iter().map(|p| effective(p)).collect();

        let mut row_data: HashMap<Key, ProposalRowChoice> = HashMap::new();
        for p in &proposal.assignments {
            let rk = Key::from(format!("{}/{}", p.remote, p.node));
            let current = effective(p);
            let bucket = if p.key.starts_with("pbs") {
                &free_pbs
            } else {
                &free_pve
            };
            let options: Vec<yew::AttrValue> = bucket
                .iter()
                .filter(|k| !taken.contains(*k) || **k == current)
                .map(|k| yew::AttrValue::from(k.clone()))
                .collect();
            let key_sockets = socket_count_from_key(&current);
            row_data.insert(
                rk,
                ProposalRowChoice {
                    options: Rc::new(options),
                    current,
                    key_sockets,
                },
            );
        }
        let row_data = Rc::new(row_data);

        let key_rows = row_data.clone();
        let link = ctx.link().clone();
        let nonces = Rc::new(self.proposal_clear_nonce.clone());
        let sock_rows = row_data;

        Rc::new(vec![
            DataTableColumn::selection_indicator().into(),
            DataTableColumn::new(tr!("Remote / Node"))
                .flex(2)
                .show_menu(false)
                .resizable(false)
                .render(|p: &ProposedAssignment| format!("{} / {}", p.remote, p.node).into())
                .into(),
            DataTableColumn::new(tr!("Key"))
                .flex(2)
                .show_menu(false)
                .resizable(false)
                .render(move |p: &ProposedAssignment| {
                    let rk_str = format!("{}/{}", p.remote, p.node);
                    let rk = Key::from(rk_str.clone());
                    let Some(choice) = key_rows.get(&rk) else {
                        return p.key.clone().into();
                    };
                    let nonce = nonces.get(&rk).copied().unwrap_or(0);
                    let vkey = Key::from(format!("{rk_str}#{nonce}"));
                    let link = link.clone();
                    let rk_cb = rk.clone();
                    let auto = p.key.clone();
                    // Picking a key and (un)ticking the row are independent affordances; wrap
                    // the cell in a click-swallowing Container so a click on the dropdown does
                    // not bubble up to the DataTable viewport's row-toggle handler and deselect
                    // the row out from under the operator.
                    Container::new()
                        .onclick(|event: MouseEvent| event.stop_propagation())
                        .with_child(
                            Combobox::new()
                                .items(choice.options.clone())
                                // Controlled value: the model is the source of truth, so the
                                // cell shows the effective key and does not emit a spurious
                                // change on mount. A clear (via Delete/Backspace) bumps the row
                                // nonce in `vkey` to remount the field on the effective key.
                                .value(Some(yew::AttrValue::from(choice.current.clone())))
                                .key(vkey)
                                .on_change(link.callback(move |val: String| {
                                    Msg::OverrideProposalKey(rk_cb.clone(), val, auto.clone())
                                })),
                        )
                        .into()
                })
                .into(),
            DataTableColumn::new(tr!("Sockets (node / key)"))
                .width("160px")
                .show_menu(false)
                .resizable(false)
                .render(move |p: &ProposedAssignment| {
                    let rk = Key::from(format!("{}/{}", p.remote, p.node));
                    let key_sockets = sock_rows.get(&rk).and_then(|c| c.key_sockets);
                    let label = match (p.node_sockets, key_sockets) {
                        (Some(ns), Some(ks)) => format!("{ns} / {ks}"),
                        (Some(ns), None) => format!("{ns} / -"),
                        (None, Some(ks)) => format!("- / {ks}"),
                        _ => String::new(),
                    };
                    label.into()
                })
                .into(),
        ])
    }

    fn adopt_columns() -> Rc<Vec<DataTableHeader<AdoptCandidate>>> {
        Rc::new(vec![
            DataTableColumn::new(tr!("Remote / Node"))
                .flex(2)
                .show_menu(false)
                .resizable(false)
                .render(|c: &AdoptCandidate| format!("{} / {}", c.remote, c.node).into())
                .into(),
            DataTableColumn::new(tr!("Key"))
                .flex(2)
                .show_menu(false)
                .resizable(false)
                .render(|c: &AdoptCandidate| c.key.clone().into())
                .into(),
        ])
    }
}

fn key_cell(n: &RemoteNodeStatus) -> Html {
    let assigned = n.assigned_key.as_deref();
    let current = n.current_key.as_deref();

    if n.pending_clear {
        // Clear queued: surface the live key the operator is about to free, with a recycle
        // icon in the warning colour so the row stands out next to ordinary pending pushes.
        let text = current.or(assigned).unwrap_or("");
        return Tooltip::new(
            Row::new()
                .class(AlignItems::Baseline)
                .gap(2)
                .with_child(Fa::new("recycle").class(FontColor::Warning))
                .with_child(text),
        )
        .tip(tr!(
            "Pending Clear - 'Apply Pending' will remove this subscription from the node."
        ))
        .into();
    }

    // Pending push = pool has a key assigned that the live state has not yet picked up. Drive
    // this off the keys themselves, not off the subscription status: a key that is on the node
    // but reports `Invalid`/`Expired`/etc. is *applied* (the push went through), just unhealthy.
    // The Status column surfaces the health axis; the clock icon here is reserved for the
    // "queued operation has not landed yet" axis.
    let pending = assigned.is_some() && current != assigned;

    match (assigned, current) {
        (Some(a), Some(c)) if a != c => Row::new()
            .class(AlignItems::Baseline)
            .gap(2)
            .with_child(Fa::new("clock-o").class(FontColor::Warning))
            .with_child(format!("{a} \u{2192} {c}"))
            .into(),
        _ => {
            let text = current.or(assigned).unwrap_or("");
            if pending {
                Row::new()
                    .class(AlignItems::Baseline)
                    .gap(2)
                    .with_child(Fa::new("clock-o").class(FontColor::Warning))
                    .with_child(text)
                    .into()
            } else if assigned.is_none() && current.is_some() {
                Tooltip::new(
                    Row::new()
                        .class(AlignItems::Baseline)
                        .gap(2)
                        .with_child(Fa::new("download").class(FontColor::Primary))
                        .with_child(text),
                )
                .tip(tr!(
                    "Not in pool - Adopt Key imports this live subscription."
                ))
                .into()
            } else {
                text.into()
            }
        }
    }
}

impl LoadableComponent for SubscriptionRegistryComp {
    type Properties = SubscriptionRegistryProps;
    type Message = Msg;
    type ViewState = ViewState;

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        let store = TreeStore::new().view_root(false);
        store.set_sorter(tree_sorter);

        let node_selection = Selection::new().on_select({
            let link = ctx.link().clone();
            move |_| link.send_redraw()
        });

        let proposal_selection = Selection::new().multiselect(true).on_select({
            let link = ctx.link().clone();
            move |_| link.send_redraw()
        });

        Self {
            state: LoadableComponentState::new(),
            tree_store: store.clone(),
            tree_columns: Self::tree_columns(store),
            proposal_selection,
            proposal_overrides: HashMap::new(),
            proposal_clear_nonce: HashMap::new(),
            adopt_columns: Self::adopt_columns(),
            node_selection,
            last_node_data: Vec::new(),
            pool_keys: Rc::new(Vec::new()),
            pool_digest: None,
            filter_expanded: false,
            node_text_filter: String::new(),
            node_status_filter: Vec::new(),
        }
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadFinished {
                nodes,
                keys,
                digest,
            } => {
                self.last_node_data = nodes.clone();
                let tree = build_tree(nodes);
                self.tree_store.write().update_root_tree(tree);
                // Re-apply the filter: its matching-remote set was computed from the previous
                // load's data and must be recomputed against the fresh statuses.
                self.apply_node_filter();
                self.pool_keys = Rc::new(keys);
                self.pool_digest = digest;
            }
            Msg::AutoAssignPreview => {
                // Switch to the computing view first so the click registers instantly; the POST
                // below fans out to every remote and may take seconds.
                ctx.link().change_view(Some(ViewState::AutoAssignComputing));
                let link = ctx.link().clone();
                ctx.link().spawn(async move {
                    match http_post::<AutoAssignProposal>(AUTO_ASSIGN_URL, None).await {
                        Ok(proposal) if proposal.assignments.is_empty() => {
                            link.show_error(
                                tr!("Auto-Assign"),
                                tr!("No suitable unassigned keys for the remaining nodes."),
                                false,
                            );
                        }
                        Ok(proposal) => link.send_message(Msg::ShowProposal(proposal)),
                        Err(err) => link.show_error(tr!("Auto-Assign"), err.to_string(), true),
                    }
                });
            }
            Msg::ShowProposal(proposal) => {
                let all_keys: HashSet<Key> = proposal
                    .assignments
                    .iter()
                    .map(|p| Key::from(format!("{}/{}", p.remote, p.node)))
                    .collect();
                self.proposal_selection.bulk_select(all_keys);
                self.proposal_overrides.clear();
                self.proposal_clear_nonce.clear();
                ctx.link()
                    .change_view(Some(ViewState::ConfirmAutoAssign(proposal)));
            }
            Msg::OverrideProposalKey(row, key, auto_pick) => {
                if key.is_empty() {
                    // Clearing the picker empties the field without changing the effective key, so
                    // the controlled value cannot resync; bump the row nonce to rebuild the cell
                    // from the auto-pick instead of leaving it blank.
                    *self.proposal_clear_nonce.entry(row.clone()).or_default() += 1;
                    self.proposal_overrides.remove(&row);
                } else if key == auto_pick {
                    // Re-picking the auto-pick (or the resync echo after a revert) just drops the
                    // override, keeping the map limited to genuine overrides.
                    self.proposal_overrides.remove(&row);
                } else {
                    self.proposal_overrides.insert(row, key);
                }
            }
            Msg::BulkAssignApply(proposal) => {
                let link = ctx.link().clone();
                ctx.link().spawn(async move {
                    let body = serde_json::json!({ "proposal": proposal });
                    match http_post::<Vec<ProposedAssignment>>(BULK_ASSIGN_URL, Some(body)).await {
                        Ok(_) => {
                            link.change_view(None);
                            link.send_reload();
                        }
                        Err(err) => link.show_error(tr!("Auto-Assign"), err.to_string(), true),
                    }
                });
            }
            Msg::ApplyPending => {
                let link = ctx.link().clone();
                let body = self
                    .pool_digest
                    .clone()
                    .map(|d| serde_json::json!({ "digest": d }));
                ctx.link().spawn(async move {
                    match http_post::<Option<String>>(APPLY_PENDING_URL, body).await {
                        // Button gated on pending != 0; None only fires on a clearing race.
                        Ok(None) => link.change_view(None),
                        Ok(Some(upid)) => {
                            link.change_view(None);
                            link.show_task_progres(upid);
                        }
                        Err(err) => link.show_error(tr!("Apply Pending"), err.to_string(), true),
                    }
                    link.send_reload();
                });
            }
            Msg::ClearPending => {
                let link = ctx.link().clone();
                let body = self
                    .pool_digest
                    .clone()
                    .map(|d| serde_json::json!({ "digest": d }));
                ctx.link().spawn(async move {
                    match http_post::<serde_json::Value>(CLEAR_PENDING_URL, body).await {
                        Ok(_) => {
                            link.change_view(None);
                            link.send_reload();
                        }
                        Err(err) => link.show_error(tr!("Discard Pending"), err.to_string(), true),
                    }
                });
            }
            Msg::RevertSelectedNode => {
                let Some(target) = self.revert_target() else {
                    return false;
                };
                let link = ctx.link().clone();
                let digest = self.pool_digest.clone();
                ctx.link().spawn(async move {
                    let err_msg: Option<String> = match target {
                        RevertTarget::Unassign(key) => {
                            let url = format!(
                                "/subscriptions/keys/{}/assignment",
                                percent_encode_component(&key),
                            );
                            let query = digest.map(|d| serde_json::json!({ "digest": d }));
                            http_delete(&url, query).await.err().map(|e| e.to_string())
                        }
                        RevertTarget::CancelClear { remote, node } => {
                            let digest = digest.map(pdm_client::ConfigDigest::from);
                            crate::pdm_client()
                                .subscription_revert_pending_clear(&remote, &node, digest)
                                .await
                                .err()
                                .map(|e| e.to_string())
                        }
                    };
                    if let Some(msg) = err_msg {
                        link.show_error(tr!("Revert"), msg, true);
                    }
                    link.send_reload();
                });
            }
            Msg::QueueClearForSelectedNode => {
                let Some((remote, node, current_key)) = self.selected_node_for_clear() else {
                    return false;
                };
                ctx.link().change_view(Some(ViewState::ConfirmQueueClear {
                    remote,
                    node,
                    current_key,
                }));
            }
            Msg::AssignKeyToSelectedNode => {
                let Some(target) = self.assign_target_for_selected_node() else {
                    return false;
                };
                ctx.link()
                    .change_view(Some(ViewState::AssignKeyToNode(target)));
            }
            Msg::AdoptKeyForSelectedNode => {
                let Some((remote, node, current_key)) = self.selected_node_for_adopt() else {
                    return false;
                };
                ctx.link().change_view(Some(ViewState::ConfirmAdoptKey {
                    remote,
                    node,
                    current_key,
                }));
            }
            Msg::AdoptAllPreview => {
                let candidates = self.adopt_all_candidates();
                if candidates.is_empty() {
                    return false;
                }
                ctx.link()
                    .change_view(Some(ViewState::ConfirmAdoptAll { candidates }));
            }
            Msg::CheckSubscriptionForSelectedNode => {
                let Some(n) = self.selected_node_status() else {
                    return false;
                };
                let remote = n.remote.clone();
                let node = n.node.clone();
                let link = ctx.link().clone();
                ctx.link().spawn(async move {
                    if let Err(err) = crate::pdm_client().subscription_check(&remote, &node).await {
                        link.show_error(tr!("Check Subscription"), err.to_string(), true);
                    }
                    link.send_reload();
                });
            }
            Msg::ToggleFilterPanel => {
                self.filter_expanded = !self.filter_expanded;
            }
            Msg::SetNodeFilterText(text) => {
                self.node_text_filter = text;
                self.apply_node_filter();
                // A filtered-out row must not stay selected and keep its actions live.
                self.node_selection.clear();
            }
            Msg::ToggleStatusFilter(status) => {
                if let Some(pos) = self.node_status_filter.iter().position(|s| *s == status) {
                    self.node_status_filter.remove(pos);
                } else {
                    self.node_status_filter.push(status);
                }
                self.apply_node_filter();
                self.node_selection.clear();
            }
            Msg::ClearStatusFilter => {
                self.node_status_filter.clear();
                self.apply_node_filter();
                self.node_selection.clear();
            }
        }
        true
    }

    fn load(
        &self,
        ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        let link = ctx.link().clone();
        Box::pin(async move {
            // Both panels share one snapshot. Fetching in parallel keeps the latency one
            // round-trip; serial would compound on slow remotes. Use `http_get_full` for the
            // pool fetch so the digest comes back alongside the entries - every mutation later
            // round-trips that digest so a stale view fails with 409 instead of overwriting a
            // parallel admin's edit.
            let nodes_fut = http_get::<Vec<RemoteNodeStatus>>(NODE_STATUS_URL, None);
            let keys_fut = http_get_full::<Vec<SubscriptionKeyEntry>>(KEYS_URL, None);
            let (nodes, keys) = futures::future::join(nodes_fut, keys_fut).await;
            let keys = keys?;
            let digest = keys
                .attribs
                .get("digest")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            link.send_message(Msg::LoadFinished {
                nodes: nodes?,
                keys: keys.data,
                digest,
            });
            Ok(())
        })
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        // Both panels are populated by one shared fetch, so a reload leaves both equally stale;
        // mask the whole content together rather than per-panel. This also covers the initial
        // load and every action that ends in `send_reload`, giving the prominent busy indicator
        // the bare refresh-button spinner lacked.
        // FlexFit on the Mask too, else its wrapper shrinks to content and the panels stop short
        // of the viewport bottom.
        Mask::new(
            Container::new()
                .class("pwt-content-spacer")
                .class(FlexFit)
                .class(FlexDirection::Row)
                .with_child(self.render_key_pool_panel(ctx))
                .with_child(self.render_node_tree_panel(ctx)),
        )
        .class(FlexFit)
        .visible(self.loading())
        .text(tr!("Loading subscription status..."))
        .into()
    }

    fn dialog_view(
        &self,
        ctx: &LoadableComponentContext<Self>,
        view_state: &Self::ViewState,
    ) -> Option<Html> {
        match view_state {
            ViewState::AutoAssignComputing => {
                use pwt::widget::Dialog;
                Some(
                    Dialog::new(tr!("Auto-Assign"))
                        .min_width(400)
                        .on_close({
                            let link = ctx.link().clone();
                            move |_| link.change_view(None)
                        })
                        .with_child(
                            Mask::new(Container::new().min_width(360).min_height(80))
                                .visible(true)
                                .text(tr!("Computing proposal...")),
                        )
                        .into(),
                )
            }
            ViewState::ConfirmApplyPending => {
                use pwt::widget::ConfirmDialog;
                let (push_count, clear_count) = self.pending_counts();
                let body = match (push_count, clear_count) {
                    (p, 0) => tr!(
                        "Push {n} queued assignment(s) to the remote nodes?",
                        n = p,
                    ),
                    (0, c) => tr!(
                        "Remove {n} live subscription(s) from the remote nodes?",
                        n = c,
                    ),
                    (p, c) => tr!(
                        "Push {p} queued assignment(s) and remove {c} live subscription(s) on the remote nodes?",
                        p = p,
                        c = c,
                    ),
                };
                Some(
                    ConfirmDialog::new(tr!("Apply Pending Changes"), body)
                        .icon_class("fa fa-question-circle")
                        .on_confirm({
                            let link = ctx.link().clone();
                            move |_| link.send_message(Msg::ApplyPending)
                        })
                        // ESC / X / No must reset the LoadableComponent's view_state too, or
                        // the dialog closes visually while the parent keeps thinking we are
                        // still on the confirm view - subsequent clicks land on a stale state.
                        .on_close({
                            let link = ctx.link().clone();
                            move |_| link.change_view(None)
                        })
                        .into(),
                )
            }
            ViewState::ConfirmClearPending => {
                use pwt::widget::ConfirmDialog;
                Some(
                    ConfirmDialog::new(
                        tr!("Discard Pending Changes"),
                        tr!("Discard all queued assignments and cancel all queued Clear Key actions? \
                             The remote nodes are not touched."),
                    )
                    .icon_class("fa fa-question-circle")
                    .on_confirm({
                        let link = ctx.link().clone();
                        move |_| link.send_message(Msg::ClearPending)
                    })
                    .on_close({
                        let link = ctx.link().clone();
                        move |_| link.change_view(None)
                    })
                    .into(),
                )
            }
            ViewState::ConfirmAutoAssign(proposal) => {
                Some(self.render_auto_assign_dialog(ctx, proposal))
            }
            ViewState::ConfirmAdoptAll { candidates } => {
                Some(self.render_adopt_all_dialog(ctx, candidates))
            }
            ViewState::ConfirmAdoptKey {
                remote,
                node,
                current_key,
            } => {
                use pwt::widget::ConfirmDialog;
                let question = tr!(
                    "Adopt {key} from {remote}/{node} into the pool?",
                    key = current_key.clone(),
                    remote = remote.clone(),
                    node = node.clone(),
                );
                let body = Column::new()
                    .gap(2)
                    .with_child(Container::from_tag("p").with_child(question))
                    .with_child(Container::from_tag("p").with_child(tr!(
                        "The live subscription is imported as a pool entry bound to this node; the remote is not contacted. After adoption the key participates in pool operations such as Clear Key and Auto-Assign."
                    )));
                let remote_for_cb = remote.clone();
                let node_for_cb = node.clone();
                let link = ctx.link().clone();
                let close_link = ctx.link().clone();
                let digest_for_cb = self.pool_digest.clone();
                Some(
                    ConfirmDialog::default()
                        .title(tr!("Adopt Key"))
                        .icon_class("fa fa-question-circle")
                        .confirm_message(body)
                        .on_confirm(move |_| {
                            let link = link.clone();
                            let remote = remote_for_cb.clone();
                            let node = node_for_cb.clone();
                            let digest = digest_for_cb.clone();
                            link.clone().spawn(async move {
                                let digest = digest.map(pdm_client::ConfigDigest::from);
                                // Reload only on success; on error keep the dialog's error visible
                                // (a trailing change_view(None) would dismiss it immediately).
                                match crate::pdm_client()
                                    .subscription_adopt_key(&remote, &node, digest)
                                    .await
                                {
                                    Ok(_) => {
                                        link.change_view(None);
                                        link.send_reload();
                                    }
                                    Err(err) => {
                                        link.show_error(tr!("Adopt Key"), err.to_string(), true)
                                    }
                                }
                            });
                        })
                        .on_close(move |_| close_link.change_view(None))
                        .into(),
                )
            }
            ViewState::ConfirmQueueClear {
                remote,
                node,
                current_key,
            } => {
                use pwt::widget::ConfirmDialog;
                let question = match current_key {
                    Some(k) => tr!(
                        "Queue a clear of {key} on {remote}/{node}?",
                        key = k.clone(),
                        remote = remote.clone(),
                        node = node.clone(),
                    ),
                    None => tr!(
                        "Queue a clear on {remote}/{node}?",
                        remote = remote.clone(),
                        node = node.clone(),
                    ),
                };
                let body = Column::new()
                    .gap(2)
                    .with_child(Container::from_tag("p").with_child(question))
                    .with_child(Container::from_tag("p").with_child(tr!(
                        "'Apply Pending' will remove the subscription from the node so the key can be reassigned elsewhere; 'Discard Pending' undoes the queueing without touching the remote."
                    )));
                let remote_for_cb = remote.clone();
                let node_for_cb = node.clone();
                let link = ctx.link().clone();
                let close_link = ctx.link().clone();
                let digest_for_cb = self.pool_digest.clone();
                Some(
                    ConfirmDialog::default()
                        .title(tr!("Clear Key"))
                        .confirm_message(body)
                        .on_confirm(move |_| {
                            let link = link.clone();
                            let remote = remote_for_cb.clone();
                            let node = node_for_cb.clone();
                            let digest = digest_for_cb.clone();
                            link.clone().spawn(async move {
                                let digest = digest.map(pdm_client::ConfigDigest::from);
                                // Reload only on success; on error keep the dialog's error visible
                                // (a trailing change_view(None) would dismiss it immediately).
                                match crate::pdm_client()
                                    .subscription_queue_clear(&remote, &node, digest)
                                    .await
                                {
                                    Ok(_) => {
                                        link.change_view(None);
                                        link.send_reload();
                                    }
                                    Err(err) => {
                                        link.show_error(tr!("Clear Key"), err.to_string(), true)
                                    }
                                }
                            });
                        })
                        .on_close(move |_| close_link.change_view(None))
                        .into(),
                )
            }
            ViewState::AssignKeyToNode(target) => {
                let close_link = ctx.link().clone();
                Some(
                    AssignKeyToNodeDialog::new(target.clone(), self.pool_keys.clone())
                        .pool_digest(self.pool_digest.clone())
                        .on_done(Callback::from(move |_| {
                            close_link.change_view(None);
                            close_link.send_reload();
                        }))
                        .into(),
                )
            }
        }
    }
}

impl SubscriptionRegistryComp {
    fn render_key_pool_panel(&self, ctx: &LoadableComponentContext<Self>) -> Panel {
        let statuses = Rc::new(self.last_node_data.clone());
        // Reload the right-side node tree whenever the left-side key pool mutates, so a fresh
        // assignment shows up as pending without forcing the operator to re-navigate.
        let reload = ctx.link().clone();
        // Both panels share one fetch, so this refresh reloads the whole view; each panel still
        // carries its own so the control sits where the operator expects it.
        let refresh = Button::refresh(self.loading()).on_activate({
            let link = ctx.link().clone();
            move |_| link.send_reload()
        });
        Panel::new()
            .class(FlexFit)
            .border(true)
            .style("flex", "3 1 0")
            .min_width(300)
            .title(tr!("Key Pool"))
            .with_tool(refresh)
            .with_child(
                SubscriptionKeyGrid::new()
                    .on_change(Callback::from(move |_| reload.send_reload()))
                    .on_auto_assign(ctx.link().callback(|_| Msg::AutoAssignPreview))
                    .node_status(statuses)
                    .pool_keys(self.pool_keys.clone())
                    .pool_digest(self.pool_digest.clone()),
            )
    }

    /// Apply the current status filter to the tree store. A node matches by status; a remote
    /// stays visible only if at least one of its nodes matches, so filtering does not leave empty
    /// remote headers behind. Recomputed from `last_node_data` whenever the filter or data change.
    fn apply_node_filter(&self) {
        let text = self.node_text_filter.trim().to_lowercase();
        let statuses = self.node_status_filter.clone();
        if text.is_empty() && statuses.is_empty() {
            self.tree_store.set_filter(None);
            return;
        }
        // A remote stays visible only when at least one of its nodes matches, so filtering does
        // not leave empty remote headers behind.
        let matching_remotes: HashSet<String> = self
            .last_node_data
            .iter()
            .filter(|n| node_matches(n, &text, &statuses))
            .map(|n| n.remote.clone())
            .collect();
        self.tree_store
            .set_filter(move |entry: &NodeTreeEntry| match entry {
                NodeTreeEntry::Root => true,
                NodeTreeEntry::Remote { name, .. } => matching_remotes.contains(name),
                NodeTreeEntry::Node { data, .. } => node_matches(data, &text, &statuses),
            });
    }

    fn render_node_tree_panel(&self, ctx: &LoadableComponentContext<Self>) -> Panel {
        let table = DataTable::new(self.tree_columns.clone(), self.tree_store.clone())
            .selection(self.node_selection.clone())
            .striped(false)
            .borderless(true)
            .show_header(true)
            .class(FlexFit);

        let can_assign_key = self.assign_target_for_selected_node().is_some();
        let can_revert = self.revert_target().is_some();
        let can_clear_key = self.selected_node_for_clear().is_some();
        let can_adopt_key = self.selected_node_for_adopt().is_some();
        let adopt_all_count = self.adopt_all_candidates().len();
        let (push_count, clear_count) = self.pending_counts();
        let pending = push_count + clear_count;
        // Check Subscription is a no-op on the remote when no key is installed (PVE / PBS
        // `update_subscription` returns early without contacting the shop), so disable the
        // button to keep the UI honest about what clicking it will do.
        let can_check = self
            .selected_node_status()
            .is_some_and(|n| n.status != proxmox_subscription::SubscriptionStatus::NotFound);
        // Common per-selection verbs are direct buttons with short labels; each tooltip says what
        // it does and when it is available, so a disabled button explains itself.
        let assign_button = Tooltip::new(
            Button::new(tr!("Assign"))
                .icon_class("fa fa-link")
                .disabled(!can_assign_key)
                .on_activate(ctx.link().callback(|_| Msg::AssignKeyToSelectedNode)),
        )
        .tip(tr!(
            "Bind a pool key to the selected node. Available for a node with no active \
             subscription and no pool assignment yet."
        ));
        let revert_button = Tooltip::new(
            Button::new(tr!("Revert"))
                .icon_class("fa fa-undo")
                .disabled(!can_revert)
                .on_activate(ctx.link().callback(|_| Msg::RevertSelectedNode)),
        )
        .tip(tr!(
            "Drop the pending pool change on the selected node. Available when the node has a \
             queued assignment or clear."
        ));
        let clear_button = Tooltip::new(
            Button::new(tr!("Clear"))
                .icon_class("fa fa-recycle")
                .disabled(!can_clear_key)
                .on_activate(ctx.link().callback(|_| Msg::QueueClearForSelectedNode)),
        )
        .tip(tr!(
            "Queue the selected node's live subscription for removal at the next Apply Pending, \
             freeing the key for reassignment. Available for a pool-managed node; for a foreign \
             subscription, adopt it first."
        ));
        let check_button = Tooltip::new(
            Button::new(tr!("Check"))
                .icon_class("fa fa-refresh")
                .disabled(!can_check)
                .on_activate(
                    ctx.link()
                        .callback(|_| Msg::CheckSubscriptionForSelectedNode),
                ),
        )
        .tip(if can_check {
            tr!("Re-verify the live subscription against the shop, refreshing its status.")
        } else {
            tr!("No subscription installed on the selected node; assign or adopt one first.")
        });

        // Both adopt scopes share one menu: import the selected node's live subscription, or every
        // untracked one across the remotes the operator can audit.
        let adopt_menu = MenuButton::new(tr!("Adopt"))
            .icon_class("fa fa-download")
            .show_arrow(true)
            .menu(
                Menu::new()
                    .with_item(
                        // No per-item icons: the menu button already carries the adopt glyph, and
                        // both scopes would otherwise share the same icon to no effect.
                        MenuItem::new(tr!("Adopt Key"))
                            .disabled(!can_adopt_key)
                            .on_select(ctx.link().callback(|_| Msg::AdoptKeyForSelectedNode)),
                    )
                    .with_item(
                        MenuItem::new(tr!("Adopt All"))
                            .disabled(adopt_all_count == 0)
                            .on_select(ctx.link().callback(|_| Msg::AdoptAllPreview)),
                    ),
            );
        let apply_pending_button = Tooltip::new(
            Button::new(tr!("Apply Pending"))
                .icon_class("fa fa-play")
                .disabled(pending == 0)
                .on_activate(
                    ctx.link()
                        .change_view_callback(|_| Some(ViewState::ConfirmApplyPending)),
                ),
        )
        .tip(tr!(
            "Push every queued assignment to its remote node and remove the \
             subscription from nodes pending clear."
        ));
        let discard_pending_button = Tooltip::new(
            Button::new(tr!("Discard Pending"))
                .icon_class("fa fa-eraser")
                .disabled(pending == 0)
                .on_activate(
                    ctx.link()
                        .change_view_callback(|_| Some(ViewState::ConfirmClearPending)),
                ),
        )
        .tip(tr!(
            "Discard queued assignments without touching the remote nodes."
        ));

        let refresh_button = Button::refresh(self.loading()).on_activate({
            let link = ctx.link().clone();
            move |_| link.send_reload()
        });

        // Filter is an icon-only funnel parked at the far right (turns primary-colored while a
        // filter is active or the panel is open), keeping the status and text filters out of the
        // verb cluster and in a collapsible section.
        let filter_active =
            !self.node_text_filter.trim().is_empty() || !self.node_status_filter.is_empty();
        let filter_button = Tooltip::new(
            Button::new_icon("fa fa-filter")
                .class((self.filter_expanded || filter_active).then_some(FontColor::Primary))
                .on_activate(ctx.link().callback(|_| Msg::ToggleFilterPanel)),
        )
        .tip(tr!(
            "Filter the node list by status or by remote and node name."
        ));

        // Left: the per-selection verbs, then the Adopt menu. Right: the queue actions and the
        // filter funnel. The pending badge is fenced off by its own rule so the verb cluster keeps
        // its position when it is absent.
        let mut toolbar = Toolbar::new()
            .border_bottom(true)
            .scroll_mode(MiniScrollMode::Arrow)
            .with_child(assign_button)
            .with_child(revert_button)
            .with_child(clear_button)
            .with_child(check_button)
            .with_spacer()
            .with_child(adopt_menu)
            .with_flex_spacer();
        if pending > 0 {
            toolbar = toolbar
                .with_child(pending_badge(push_count, clear_count))
                .with_spacer();
        }
        toolbar = toolbar
            .with_child(apply_pending_button)
            .with_child(discard_pending_button)
            .with_spacer()
            .with_child(filter_button);

        // Collapsible filter row, CSS-hidden when closed so the field keeps focus and state across
        // toggles (the task-view filter idiom): a free-text remote/node field plus a multi-select
        // segmented status filter whose "All" segment clears it.
        let statuses = &self.node_status_filter;
        let mut status_filter = SegmentedButton::new()
            .class("pwt-button-elevated")
            .with_button(
                Button::new(tr!("All"))
                    .pressed(statuses.is_empty())
                    .class(
                        statuses
                            .is_empty()
                            .then_some(ColorScheme::SecondaryContainer),
                    )
                    .on_activate(ctx.link().callback(|_| Msg::ClearStatusFilter)),
            );
        for status in all_node_statuses() {
            let active = statuses.contains(&status);
            status_filter = status_filter.with_button(
                Button::new(subscription_status_label(status))
                    .pressed(active)
                    .class(active.then_some(ColorScheme::SecondaryContainer))
                    .on_activate(
                        ctx.link()
                            .callback(move |_| Msg::ToggleStatusFilter(status)),
                    ),
            );
        }
        let filter_panel = Row::new()
            .border_bottom(true)
            .class(AlignItems::Center)
            .class((!self.filter_expanded).then_some(Display::None))
            .padding(2)
            .gap(2)
            .with_child(
                Field::new()
                    .placeholder(tr!("Filter by remote or node"))
                    .attribute("aria-label", tr!("Filter nodes by remote or node name"))
                    .value(self.node_text_filter.clone())
                    .with_trigger(
                        Trigger::new(if self.node_text_filter.is_empty() {
                            ""
                        } else {
                            "fa fa-times"
                        })
                        .tip(tr!("Clear filter"))
                        .on_activate(
                            ctx.link()
                                .callback(|_| Msg::SetNodeFilterText(String::new())),
                        ),
                        true,
                    )
                    .on_input(ctx.link().callback(Msg::SetNodeFilterText)),
            )
            .with_child(status_filter);

        // Show a hint, not an empty grid, when no nodes exist at all (raw data, ignoring filters).
        let node_body: Html = if self.last_node_data.is_empty() {
            empty_state_hint("server", tr!("No remote nodes available yet."))
        } else {
            table.into()
        };

        Panel::new()
            .class(FlexFit)
            .border(true)
            .style("flex", "4 1 0")
            .min_width(400)
            .title(tr!("Node Subscription Status"))
            .with_tool(refresh_button)
            .with_child(
                Column::new()
                    .class(FlexFit)
                    .with_child(toolbar)
                    .with_child(filter_panel)
                    .with_child(node_body),
            )
    }

    /// Return `(pending pushes, pending clears)` mirroring the server's `compute_pending`
    /// predicate. Iterates the pool (not the node-status list) so a pool entry bound to a
    /// vanished node still counts as pending - matching what Apply Pending would actually try.
    fn pending_counts(&self) -> (u32, u32) {
        let mut push = 0;
        let mut clear = 0;
        for entry in self.pool_keys.iter() {
            let (Some(remote), Some(node)) = (entry.remote.as_deref(), entry.node.as_deref())
            else {
                continue;
            };
            if entry.pending_clear {
                clear += 1;
                continue;
            }
            // Pending push = the live current key on the node does not match the assigned pool
            // key. Subscription health (Invalid, Expired, ...) is a separate axis surfaced via
            // the Status column; re-pushing the same key would not change the shop's verdict
            // and the badge must not double-count health issues as queued operations.
            let is_pending = match self
                .last_node_data
                .iter()
                .find(|n| n.remote == remote && n.node == node)
            {
                Some(n) => n.current_key.as_deref() != Some(entry.key.as_str()),
                None => true,
            };
            if is_pending {
                push += 1;
            }
        }
        (push, clear)
    }

    /// Resolve the selected tree row to its `RemoteNodeStatus`, if any.
    fn selected_node_status(&self) -> Option<&RemoteNodeStatus> {
        let key = self.node_selection.selected_key()?;
        let raw = key.to_string();
        let mut parts = raw.trim_start_matches('/').splitn(2, '/');
        let remote = parts.next()?;
        let node = parts.next()?;
        self.last_node_data
            .iter()
            .find(|n| n.remote == remote && n.node == node)
    }

    /// Resolve the selected node into a Revert action target.
    ///
    /// Revertible: an unpushed pool assignment (drop the binding) or a queued Clear Key (drop the
    /// flag). An already-applied binding - live key matches the assignment, even if the
    /// subscription is Expired/Invalid - has no pending push to revert; freeing it is Clear Key.
    fn revert_target(&self) -> Option<RevertTarget> {
        let n = self.selected_node_status()?;
        if n.pending_clear {
            return Some(RevertTarget::CancelClear {
                remote: n.remote.clone(),
                node: n.node.clone(),
            });
        }
        let assigned = n.assigned_key.as_ref()?;
        if n.current_key.as_deref() == Some(assigned.as_str()) {
            return None;
        }
        Some(RevertTarget::Unassign(assigned.clone()))
    }

    /// Returns `(remote, node, current_key)` when the selected node has a pool-managed
    /// subscription that can be queued for clear: there is a live key, no clear is already
    /// queued for it, and a pool entry is bound to (remote, node). The pool-binding gate
    /// mirrors the server-side refusal so foreign live subscriptions do not offer Clear Key
    /// (they need Adopt Key first).
    fn selected_node_for_clear(&self) -> Option<(String, String, Option<String>)> {
        let n = self.selected_node_status()?;
        if n.pending_clear || n.current_key.is_none() || n.assigned_key.is_none() {
            return None;
        }
        Some((n.remote.clone(), n.node.clone(), n.current_key.clone()))
    }

    /// Returns `(remote, node, current_key)` when the selected node has a foreign live
    /// subscription eligible for Adopt Key: a current key is set on the node and no pool entry
    /// is bound to (remote, node) yet. Mutually exclusive with `selected_node_for_clear` so the
    /// toolbar can offer exactly one of Clear Key / Adopt Key for any given selection.
    fn selected_node_for_adopt(&self) -> Option<(String, String, String)> {
        let n = self.selected_node_status()?;
        if n.assigned_key.is_some() {
            return None;
        }
        let current_key = n.current_key.clone()?;
        Some((n.remote.clone(), n.node.clone(), current_key))
    }

    /// Iterate the loaded node-status snapshot and return every `(remote, node, current_key)`
    /// eligible for bulk Adopt-All (live key set, no pool binding). Used both for the toolbar
    /// disabled gate and for the preview list in the confirm dialog; the authoritative set is
    /// recomputed by the server under the lock at commit time, so this view is a hint, not a
    /// contract.
    fn adopt_all_candidates(&self) -> Vec<(String, String, String)> {
        self.last_node_data
            .iter()
            .filter_map(|n| {
                if n.assigned_key.is_some() {
                    return None;
                }
                n.current_key
                    .clone()
                    .map(|k| (n.remote.clone(), n.node.clone(), k))
            })
            .collect()
    }

    /// Returns the [`AssignTarget`] for the right-panel Assign button: selected row is a node, no
    /// assigned key in the pool yet, and no live active subscription. Refusing earlier than the
    /// server keeps the button-disable affordance honest.
    fn assign_target_for_selected_node(&self) -> Option<AssignTarget> {
        let n = self.selected_node_status()?;
        if n.assigned_key.is_some() {
            return None;
        }
        if n.status == proxmox_subscription::SubscriptionStatus::Active {
            return None;
        }
        Some(AssignTarget {
            remote: n.remote.clone(),
            node: n.node.clone(),
            ty: n.ty,
            sockets: n.sockets,
        })
    }

    fn render_auto_assign_dialog(
        &self,
        ctx: &LoadableComponentContext<Self>,
        proposal: &AutoAssignProposal,
    ) -> Html {
        use pwt::widget::Dialog;

        let store: Store<ProposedAssignment> = Store::with_extract_key(|p: &ProposedAssignment| {
            format!("{}/{}", p.remote, p.node).into()
        });
        store.set_data(proposal.assignments.clone());

        let total = proposal.assignments.len();
        let selected_count = self.proposal_selection.len();

        let link_close = ctx.link().clone();
        let link_apply = ctx.link().clone();
        let selection = self.proposal_selection.clone();
        let overrides = self.proposal_overrides.clone();
        let proposal_for_apply = proposal.clone();
        let body = Column::new()
            .class(Flex::Fill)
            .class(Overflow::Hidden)
            .min_height(0)
            .padding(2)
            .gap(2)
            .min_width(600)
            .with_child(Container::from_tag("p").with_child(tr!(
                "{selected} of {total} assignments selected. Untick any node to skip it or pick a different key, then click Assign.",
                selected = selected_count,
                total = total,
            )))
            .with_child(
                DataTable::new(self.proposal_columns(ctx, proposal), store)
                    .selection(self.proposal_selection.clone())
                    .multiselect_mode(MultiSelectMode::Simple)
                    .striped(true)
                    .class(FlexFit)
                    .min_height(140),
            )
            .with_child(
                Row::new()
                    .class(JustifyContent::FlexEnd)
                    .gap(2)
                    .padding_top(2)
                    .with_child(
                        Button::new(tr!("Cancel"))
                            .on_activate(move |_| link_close.change_view(None)),
                    )
                    .with_child(
                        Button::new(tr!("Assign"))
                            .disabled(selected_count == 0)
                            .on_activate(move |_| {
                                let selected: HashSet<Key> =
                                    selection.selected_keys().into_iter().collect();
                                let mut filtered = proposal_for_apply.clone();
                                filtered.assignments.retain(|p| {
                                    selected.contains(&Key::from(format!("{}/{}", p.remote, p.node)))
                                });
                                // Swap in any pinned key so the override, not the auto-pick, is
                                // what gets bound. bulk-assign keys off remote/node/key only, so
                                // the stale key_sockets does not matter.
                                for a in filtered.assignments.iter_mut() {
                                    let rk = Key::from(format!("{}/{}", a.remote, a.node));
                                    if let Some(key) = overrides.get(&rk) {
                                        a.key = key.clone();
                                    }
                                }
                                link_apply.send_message(Msg::BulkAssignApply(filtered));
                            }),
                    ),
            );

        Dialog::new(tr!("Auto-Assign Proposal"))
            .resizable(true)
            .width(700)
            .min_width(500)
            .min_height(300)
            .max_height("80vh")
            .on_close({
                let link = ctx.link().clone();
                move |_| link.change_view(None)
            })
            .with_child(body)
            .into()
    }

    fn render_adopt_all_dialog(
        &self,
        ctx: &LoadableComponentContext<Self>,
        candidates: &[(String, String, String)],
    ) -> Html {
        use pwt::widget::Dialog;

        let rows: Vec<AdoptCandidate> = candidates
            .iter()
            .map(|(r, n, k)| AdoptCandidate {
                remote: r.clone(),
                node: n.clone(),
                key: k.clone(),
            })
            .collect();
        let n = rows.len();
        let store: Store<AdoptCandidate> =
            Store::with_extract_key(|c: &AdoptCandidate| format!("{}/{}", c.remote, c.node).into());
        store.set_data(rows);

        let link_close = ctx.link().clone();
        let link_apply = ctx.link().clone();
        let digest = self.pool_digest.clone();
        let body = Column::new()
            .class(Flex::Fill)
            .class(Overflow::Hidden)
            .min_height(0)
            .padding(2)
            .gap(2)
            .min_width(600)
            .with_child(Container::from_tag("p").with_child(tr!(
                "The following {n} live subscription(s) will be imported into the pool; \
                 the remote is not contacted.",
                n = n,
            )))
            .with_child(
                DataTable::new(self.adopt_columns.clone(), store)
                    .striped(true)
                    .class(FlexFit)
                    .min_height(140),
            )
            .with_child(
                Row::new()
                    .class(JustifyContent::FlexEnd)
                    .gap(2)
                    .padding_top(2)
                    .with_child(
                        Button::new(tr!("Cancel"))
                            .on_activate(move |_| link_close.change_view(None)),
                    )
                    .with_child(Button::new(tr!("Adopt")).on_activate(move |_| {
                        let link = link_apply.clone();
                        let digest = digest.clone();
                        link.clone().spawn(async move {
                            let digest = digest.map(pdm_client::ConfigDigest::from);
                            // Reload only on success; on error keep the dialog's error visible
                            // (a trailing change_view(None) would dismiss it immediately).
                            match crate::pdm_client().subscription_adopt_all(digest).await {
                                Ok(_) => {
                                    link.change_view(None);
                                    link.send_reload();
                                }
                                Err(err) => {
                                    link.show_error(tr!("Adopt All"), err.to_string(), true)
                                }
                            }
                        });
                    })),
            );

        Dialog::new(tr!("Adopt All"))
            .resizable(true)
            .width(700)
            .min_width(500)
            .min_height(300)
            .max_height("80vh")
            .on_close({
                let link = ctx.link().clone();
                move |_| link.change_view(None)
            })
            .with_child(body)
            .into()
    }
}
