use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;

use yew::virtual_dom::{Key, VComp, VNode};

use proxmox_yew_comp::percent_encoding::percent_encode_component;
use proxmox_yew_comp::{http_delete, http_get, http_get_full, http_post};
use proxmox_yew_comp::{
    LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState,
};

use pwt::css::{AlignItems, Flex, FlexDirection, FlexFit, FontColor, JustifyContent, Overflow};
use pwt::prelude::*;
use pwt::props::{ContainerBuilder, ExtractPrimaryKey, WidgetBuilder};
use pwt::state::{Selection, SlabTree, Store, TreeStore};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::{Button, Column, Container, Fa, Mask, Panel, Row, Toolbar, Tooltip};

use pdm_api_types::subscription::{
    AutoAssignProposal, ProposedAssignment, RemoteNodeStatus, SubscriptionKeyEntry,
    SubscriptionLevel,
};

use super::subscription_assign::{AssignKeyToNodeDialog, AssignTarget};
use super::subscription_keys::SubscriptionKeyGrid;

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
    proposal_columns: Rc<Vec<DataTableHeader<ProposedAssignment>>>,
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
                    NodeTreeEntry::Node { data: n, .. } => Row::new()
                        .class(AlignItems::Baseline)
                        .gap(2)
                        .with_child(subscription_status_icon(n.status))
                        .with_child(subscription_status_label(n.status))
                        .into(),
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

    // Read-only preview grid: menu and resize are independent flags, disable both.
    fn proposal_columns() -> Rc<Vec<DataTableHeader<ProposedAssignment>>> {
        Rc::new(vec![
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
                .render(|p: &ProposedAssignment| p.key.clone().into())
                .into(),
            DataTableColumn::new(tr!("Sockets (node / key)"))
                .width("160px")
                .show_menu(false)
                .resizable(false)
                .render(|p: &ProposedAssignment| {
                    let label = match (p.node_sockets, p.key_sockets) {
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

        Self {
            state: LoadableComponentState::new(),
            tree_store: store.clone(),
            tree_columns: Self::tree_columns(store),
            proposal_columns: Self::proposal_columns(),
            adopt_columns: Self::adopt_columns(),
            node_selection,
            last_node_data: Vec::new(),
            pool_keys: Rc::new(Vec::new()),
            pool_digest: None,
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
                        Ok(proposal) => {
                            link.change_view(Some(ViewState::ConfirmAutoAssign(proposal)));
                        }
                        Err(err) => link.show_error(tr!("Auto-Assign"), err.to_string(), true),
                    }
                });
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
        }
        true
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        let link = ctx.link();
        let (push_count, clear_count) = self.pending_counts();
        let adopt_all_count = self.adopt_all_candidates().len();
        let mut toolbar = Toolbar::new()
            .border_bottom(true)
            .with_child(
                Tooltip::new(
                    Button::new(tr!("Auto-Assign"))
                        .icon_class("fa fa-magic")
                        .on_activate(link.callback(|_| Msg::AutoAssignPreview)),
                )
                .tip(tr!(
                    "Propose a one-key-per-node assignment for nodes that have no active \
                     subscription, then queue it pending Apply."
                )),
            )
            .with_child(
                Tooltip::new(
                    Button::new(tr!("Adopt All"))
                        .icon_class("fa fa-download")
                        .disabled(adopt_all_count == 0)
                        .on_activate(link.callback(|_| Msg::AdoptAllPreview)),
                )
                .tip(tr!(
                    "Import every foreign live subscription that is not yet tracked by the \
                     pool. The remote is not contacted; only the pool config is updated."
                )),
            )
            .with_spacer()
            .with_child(
                Tooltip::new(
                    Button::new(tr!("Apply Pending"))
                        .icon_class("fa fa-play")
                        .disabled(push_count + clear_count == 0)
                        .on_activate(
                            link.change_view_callback(|_| Some(ViewState::ConfirmApplyPending)),
                        ),
                )
                .tip(tr!(
                    "Push every queued assignment to its remote node and remove the \
                     subscription from nodes pending clear."
                )),
            )
            .with_child(
                Tooltip::new(
                    Button::new(tr!("Discard Pending"))
                        .icon_class("fa fa-eraser")
                        .disabled(push_count + clear_count == 0)
                        .on_activate(
                            link.change_view_callback(|_| Some(ViewState::ConfirmClearPending)),
                        ),
                )
                .tip(tr!(
                    "Discard queued assignments without touching the remote nodes."
                )),
            )
            .with_flex_spacer();

        if push_count + clear_count > 0 {
            toolbar = toolbar.with_child(pending_badge(push_count, clear_count));
        }

        Some(
            toolbar
                .with_flex_spacer()
                .with_child(Button::refresh(self.loading()).on_activate({
                    let link = link.clone();
                    move |_| link.send_reload()
                }))
                .into(),
        )
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
                                if let Err(err) = crate::pdm_client()
                                    .subscription_adopt_key(&remote, &node, digest)
                                    .await
                                {
                                    link.show_error(tr!("Adopt Key"), err.to_string(), true);
                                }
                                link.change_view(None);
                                link.send_reload();
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
                                if let Err(err) = crate::pdm_client()
                                    .subscription_queue_clear(&remote, &node, digest)
                                    .await
                                {
                                    link.show_error(tr!("Clear Key"), err.to_string(), true);
                                }
                                link.change_view(None);
                                link.send_reload();
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
        // Reload the right-side node tree whenever the left-side key pool mutates, so a fresh
        // assignment shows up as pending without forcing the operator to re-navigate.
        let link = ctx.link().clone();
        // Pass the current node-status snapshot into the grid so its Clear button can be
        // disabled for synced bindings (orphan-prevention - mirrors the server-side refusal).
        let statuses = Rc::new(self.last_node_data.clone());
        Panel::new()
            .class(FlexFit)
            .border(true)
            .style("flex", "3 1 0")
            .min_width(300)
            .title(tr!("Key Pool"))
            .with_child(
                SubscriptionKeyGrid::new()
                    .on_change(Callback::from(move |_| link.send_reload()))
                    .node_status(statuses)
                    .pool_keys(self.pool_keys.clone())
                    .pool_digest(self.pool_digest.clone()),
            )
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
        let assign_button = Tooltip::new(
            Button::new(tr!("Assign Key"))
                .icon_class("fa fa-link")
                .disabled(!can_assign_key)
                .on_activate(ctx.link().callback(|_| Msg::AssignKeyToSelectedNode)),
        )
        .tip(tr!(
            "Bind a pool key to the selected node. Available for nodes without an active \
             subscription that have no pool assignment yet."
        ));
        let revert_button = Tooltip::new(
            Button::new(tr!("Revert"))
                .icon_class("fa fa-undo")
                .disabled(!can_revert)
                .on_activate(ctx.link().callback(|_| Msg::RevertSelectedNode)),
        )
        .tip(tr!("Drop the pending pool change on the selected node."));
        let clear_key_button = Tooltip::new(
            Button::new(tr!("Clear Key"))
                .icon_class("fa fa-recycle")
                .disabled(!can_clear_key)
                .on_activate(ctx.link().callback(|_| Msg::QueueClearForSelectedNode)),
        )
        .tip(tr!(
            "Queue the live subscription on the selected node for removal at next Apply \
             Pending, freeing the key for reassignment. Requires the node to be \
             pool-managed; for foreign subscriptions, run Adopt Key first."
        ));
        let adopt_key_button = Tooltip::new(
            Button::new(tr!("Adopt Key"))
                .icon_class("fa fa-download")
                .disabled(!can_adopt_key)
                .on_activate(ctx.link().callback(|_| Msg::AdoptKeyForSelectedNode)),
        )
        .tip(tr!(
            "Import the live subscription on the selected node into the pool."
        ));

        Panel::new()
            .class(FlexFit)
            .border(true)
            .style("flex", "4 1 0")
            .min_width(400)
            .title(tr!("Node Subscription Status"))
            .with_tool(assign_button)
            .with_tool(adopt_key_button)
            .with_tool(revert_button)
            .with_tool(clear_key_button)
            .with_child(table)
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

        let link_close = ctx.link().clone();
        let link_apply = ctx.link().clone();
        let proposal_for_apply = proposal.clone();
        let body = Column::new()
            .class(Flex::Fill)
            .class(Overflow::Hidden)
            .min_height(0)
            .padding(2)
            .gap(2)
            .min_width(600)
            .with_child(Container::from_tag("p").with_child(tr!(
                "The following {n} assignments are proposed. Click Assign to confirm.",
                n = proposal.assignments.len(),
            )))
            .with_child(
                DataTable::new(self.proposal_columns.clone(), store)
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
                    .with_child(Button::new(tr!("Assign")).on_activate(move |_| {
                        link_apply.send_message(Msg::BulkAssignApply(proposal_for_apply.clone()))
                    })),
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
                            if let Err(err) =
                                crate::pdm_client().subscription_adopt_all(digest).await
                            {
                                link.show_error(tr!("Adopt All"), err.to_string(), true);
                            }
                            link.change_view(None);
                            link.send_reload();
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
