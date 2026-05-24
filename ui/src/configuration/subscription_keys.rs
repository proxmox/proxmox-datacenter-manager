use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;

use pdm_api_types::remotes::RemoteType;
use pdm_api_types::subscription::{
    AddKeysResult, ProductType, RemoteNodeStatus, SubscriptionKeyEntry, SubscriptionKeySource,
    SUBSCRIPTION_KEY_SCHEMA,
};
use yew::virtual_dom::{Key, VComp, VNode};

use proxmox_yew_comp::percent_encoding::percent_encode_component;
use proxmox_yew_comp::{http_delete, http_post, EditWindow};
use proxmox_yew_comp::{
    LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState,
};

use pwt::css::{FontColor, FontStyle, Opacity};
use pwt::prelude::*;
use pwt::state::{Selection, Store};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::{DisplayField, FormContext, TextArea};
use pwt::widget::{Button, ConfirmDialog, Container, InputPanel, Toolbar, Tooltip};
use pwt_macros::builder;
use yew::html::IntoEventCallback;

use crate::widget::{PveNodeSelector, RemoteSelector};

const BASE_URL: &str = "/subscriptions/keys";

#[derive(Properties, PartialEq, Clone)]
#[builder]
pub struct SubscriptionKeyGrid {
    /// Pool keys, owned by the parent registry so both panels see the same snapshot.
    #[builder]
    #[prop_or_default]
    pub pool_keys: Rc<Vec<SubscriptionKeyEntry>>,

    /// Pool-config digest captured by the parent registry on its last `/subscriptions/keys`
    /// fetch. Passed through to every mutation so the server can reject (409) a call made
    /// against a stale view rather than silently overwriting a parallel admin's edits.
    #[builder]
    #[prop_or_default]
    pub pool_digest: Option<String>,

    /// Called after every successful pool mutation (add, assign, clear, remove). Lets the parent
    /// view (the Subscription Registry) reload its own data so the Node Status side stays in
    /// sync with the Key Pool side.
    #[builder_cb(IntoEventCallback, into_event_callback, ())]
    #[prop_or_default]
    pub on_change: Option<Callback<()>>,

    /// Latest live node-status snapshot from the parent view. Used to disable the Clear button
    /// when the selected entry's binding is currently synced (the assigned key is the live
    /// active key on its remote), since unassigning then would orphan the live subscription.
    /// The server enforces the same gate; this prop just turns it into a UI affordance.
    #[builder]
    #[prop_or_default]
    pub node_status: Rc<Vec<RemoteNodeStatus>>,

    /// When set, a right-aligned Auto-Assign button in the toolbar drives this callback.
    #[builder_cb(IntoEventCallback, into_event_callback, ())]
    #[prop_or_default]
    pub on_auto_assign: Option<Callback<()>>,
}

impl SubscriptionKeyGrid {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

impl Default for SubscriptionKeyGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl From<SubscriptionKeyGrid> for VNode {
    fn from(val: SubscriptionKeyGrid) -> Self {
        VComp::new::<LoadableComponentMaster<SubscriptionKeyGridComp>>(Rc::new(val), None).into()
    }
}

pub enum Msg {
    Remove(Key),
    Reload,
}

#[derive(PartialEq)]
pub enum ViewState {
    Add,
    Assign,
    Remove,
}

#[doc(hidden)]
pub struct SubscriptionKeyGridComp {
    state: LoadableComponentState<ViewState>,
    store: Store<SubscriptionKeyEntry>,
    columns: Rc<Vec<DataTableHeader<SubscriptionKeyEntry>>>,
    selection: Selection,
}

pwt::impl_deref_mut_property!(
    SubscriptionKeyGridComp,
    state,
    LoadableComponentState<ViewState>
);

impl SubscriptionKeyGridComp {
    fn columns() -> Rc<Vec<DataTableHeader<SubscriptionKeyEntry>>> {
        Rc::new(vec![
            DataTableColumn::new(tr!("Key"))
                .flex(2)
                .sorter(|a: &SubscriptionKeyEntry, b: &SubscriptionKeyEntry| a.key.cmp(&b.key))
                .sort_order(true)
                .render(|entry: &SubscriptionKeyEntry| entry.key.as_str().into())
                .into(),
            DataTableColumn::new(tr!("Product"))
                .width("80px")
                .sorter(|a: &SubscriptionKeyEntry, b: &SubscriptionKeyEntry| {
                    a.product_type.to_string().cmp(&b.product_type.to_string())
                })
                .render(|entry: &SubscriptionKeyEntry| entry.product_type.to_string().into())
                .into(),
            DataTableColumn::new(tr!("Level"))
                .width("100px")
                .sorter(|a: &SubscriptionKeyEntry, b: &SubscriptionKeyEntry| a.level.cmp(&b.level))
                .render(|entry: &SubscriptionKeyEntry| entry.level.to_string().into())
                .into(),
            DataTableColumn::new(tr!("Source"))
                .width("90px")
                .hidden(true)
                .sorter(|a: &SubscriptionKeyEntry, b: &SubscriptionKeyEntry| {
                    (a.source as u8).cmp(&(b.source as u8))
                })
                .render(|entry: &SubscriptionKeyEntry| match entry.source {
                    SubscriptionKeySource::Manual => tr!("Manual").into(),
                    SubscriptionKeySource::Adopted => tr!("Adopted").into(),
                })
                .into(),
            DataTableColumn::new(tr!("Assignment"))
                .flex(2)
                .sorter(|a: &SubscriptionKeyEntry, b: &SubscriptionKeyEntry| {
                    (&a.remote, &a.node).cmp(&(&b.remote, &b.node))
                })
                .render(
                    |entry: &SubscriptionKeyEntry| match (&entry.remote, &entry.node) {
                        (Some(remote), Some(node)) => format!("{remote} / {node}").into(),
                        _ => Html::default(),
                    },
                )
                .into(),
        ])
    }

    fn selected_entry(&self) -> Option<SubscriptionKeyEntry> {
        let key = self.selection.selected_key()?;
        self.store.read().lookup_record(&key).cloned()
    }

    fn create_add_dialog(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        let digest = ctx.props().pool_digest.clone();
        EditWindow::new(tr!("Add Subscription Keys"))
            .renderer(|form_ctx| add_input_panel(form_ctx))
            .on_submit(move |form| submit_add_keys(form, digest.clone()))
            .on_done(ctx.link().clone().callback(|_| Msg::Reload))
            .into()
    }

    fn create_assign_dialog(
        &self,
        entry: &SubscriptionKeyEntry,
        ctx: &LoadableComponentContext<Self>,
    ) -> Html {
        let key = entry.key.clone();
        let product_type = entry.product_type;
        let node_status = ctx.props().node_status.clone();
        let digest = ctx.props().pool_digest.clone();
        EditWindow::new(tr!("Assign Key to Remote"))
            .renderer({
                let key = key.clone();
                move |form_ctx| assign_input_panel(&key, product_type, form_ctx, &node_status)
            })
            .on_submit({
                let key = key.clone();
                move |form| submit_assign(key.clone(), form, digest.clone())
            })
            .on_done(ctx.link().clone().callback(|_| Msg::Reload))
            .into()
    }
}

impl LoadableComponent for SubscriptionKeyGridComp {
    type Properties = SubscriptionKeyGrid;
    type Message = Msg;
    type ViewState = ViewState;

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        let selection = Selection::new().on_select({
            let link = ctx.link().clone();
            move |_| link.send_redraw()
        });
        let store =
            Store::with_extract_key(|entry: &SubscriptionKeyEntry| entry.key.as_str().into());
        store.set_data((*ctx.props().pool_keys).clone());
        Self {
            state: LoadableComponentState::new(),
            store,
            columns: Self::columns(),
            selection,
        }
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Remove(key) => {
                let id = key.to_string();
                let link = ctx.link().clone();
                let digest = ctx.props().pool_digest.clone();
                ctx.link().spawn(async move {
                    let url = format!("{BASE_URL}/{}", percent_encode_component(&id));
                    let query = digest.map(|d| serde_json::json!({ "digest": d }));
                    if let Err(err) = http_delete(&url, query).await {
                        link.show_error(
                            tr!("Error"),
                            tr!("Could not remove {id}: {err}", id = id, err = err),
                            true,
                        );
                    }
                    link.send_message(Msg::Reload);
                });
            }
            Msg::Reload => {
                ctx.link().change_view(None);
                if let Some(cb) = &ctx.props().on_change {
                    cb.emit(());
                }
            }
        }
        true
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        let entry = self.selected_entry();
        let has_selection = entry.is_some();
        let is_assigned = entry.as_ref().map(|e| e.remote.is_some()).unwrap_or(false);
        let synced_assignment = entry
            .as_ref()
            .map(|e| is_synced_assignment(e, &ctx.props().node_status))
            .unwrap_or(false);
        let assignable = entry
            .as_ref()
            .map(|e| {
                e.product_type.matches_remote_type(RemoteType::Pve)
                    || e.product_type.matches_remote_type(RemoteType::Pbs)
            })
            .unwrap_or(false);
        let link = ctx.link();

        let mut toolbar = Toolbar::new()
            .border_bottom(true)
            .with_child(
                Tooltip::new(
                    Button::new(tr!("Add"))
                        .icon_class("fa fa-plus")
                        .on_activate(link.change_view_callback(|_| Some(ViewState::Add))),
                )
                .tip(tr!(
                    "Add one or more subscription keys to the pool; the Assign step \
                     happens later."
                )),
            )
            .with_spacer()
            .with_child(
                Tooltip::new(
                    Button::new(tr!("Assign"))
                        .icon_class("fa fa-link")
                        .disabled(!has_selection || is_assigned || !assignable)
                        .on_activate(link.change_view_callback(|_| Some(ViewState::Assign))),
                )
                .tip(tr!(
                    "Pin the selected key to a remote node; Apply Pending pushes the \
                     assignment to the remote."
                )),
            )
            .with_child(
                Tooltip::new(
                    Button::new(tr!("Remove Key"))
                        .icon_class("fa fa-trash-o")
                        .disabled(!has_selection || synced_assignment)
                        .on_activate(link.change_view_callback(|_| Some(ViewState::Remove))),
                )
                .tip(tr!(
                    "Remove the selected key from the pool. Disabled while the key is \
                     live on a remote node."
                )),
            );

        if let Some(cb) = ctx.props().on_auto_assign.clone() {
            toolbar = toolbar.with_flex_spacer().with_child(
                Tooltip::new(
                    Button::new(tr!("Auto-Assign"))
                        .icon_class("fa fa-magic")
                        .on_activate(move |_| cb.emit(())),
                )
                .tip(tr!(
                    "Propose a one-key-per-node assignment for nodes that have no active \
                     subscription, then queue it pending Apply."
                )),
            );
        }

        Some(toolbar.into())
    }

    fn changed(
        &mut self,
        ctx: &LoadableComponentContext<Self>,
        old_props: &Self::Properties,
    ) -> bool {
        if !Rc::ptr_eq(&old_props.pool_keys, &ctx.props().pool_keys) {
            self.store.set_data((*ctx.props().pool_keys).clone());
        }
        true
    }

    fn load(
        &self,
        _ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        // Pool data flows in via the `pool_keys` prop owned by the parent registry; the grid
        // does not fetch on its own. Resolve immediately so the LoadableComponent harness does
        // not show its mask.
        Box::pin(async { Ok(()) })
    }

    fn main_view(&self, _ctx: &LoadableComponentContext<Self>) -> Html {
        DataTable::new(self.columns.clone(), self.store.clone())
            .selection(self.selection.clone())
            .into()
    }

    fn dialog_view(
        &self,
        ctx: &LoadableComponentContext<Self>,
        view_state: &Self::ViewState,
    ) -> Option<Html> {
        match view_state {
            ViewState::Add => Some(self.create_add_dialog(ctx)),
            ViewState::Assign => self
                .selected_entry()
                .map(|entry| self.create_assign_dialog(&entry, ctx)),
            ViewState::Remove => self.selection.selected_key().map(|key| {
                let assignment = self.selected_entry().and_then(|e| {
                    Some((e.remote.clone()?, e.node.clone()?))
                });
                let body = match assignment {
                    Some((remote, node)) => tr!(
                        "Remove {key} from the key pool? It is still assigned to {remote}/{node}; the assignment is released without removing any subscription on the remote. Use Clear Key on the Node Subscription Status panel first to release a live subscription on that node too.",
                        key = key.to_string(),
                        remote = remote,
                        node = node,
                    ),
                    None => tr!(
                        "Remove {key} from the key pool? This does not revoke the subscription.",
                        key = key.to_string(),
                    ),
                };
                ConfirmDialog::new(tr!("Remove Key"), body)
                    .on_confirm({
                        let link = ctx.link().clone();
                        let key = key.clone();
                        move |_| link.send_message(Msg::Remove(key.clone()))
                    })
                    .on_close({
                        let link = ctx.link().clone();
                        move |_| link.change_view(None)
                    })
                    .into()
            }),
        }
    }
}

/// Returns true when the pool entry's binding currently runs the same key on the remote and is
/// Active - meaning a clear-assignment would orphan the live subscription. Mirrors the
/// server-side gate; the operator must run Clear Key on the Node Subscription Status panel
/// first.
fn is_synced_assignment(entry: &SubscriptionKeyEntry, statuses: &[RemoteNodeStatus]) -> bool {
    let (Some(remote), Some(node)) = (entry.remote.as_deref(), entry.node.as_deref()) else {
        return false;
    };
    statuses
        .iter()
        .find(|n| n.remote == remote && n.node == node)
        .map(|n| {
            n.status == proxmox_subscription::SubscriptionStatus::Active
                && n.current_key.as_deref() == Some(entry.key.as_str())
        })
        .unwrap_or(false)
}

fn add_input_panel(form_ctx: &FormContext) -> Html {
    let hint = Container::new()
        .class(FontStyle::TitleSmall)
        .class(Opacity::ThreeQuarters)
        .padding_top(2)
        .with_child(tr!(
            "One key per line, or comma-separated. Only Proxmox VE and Proxmox Backup Server keys are accepted."
        ));

    // A TextArea only toggles a red border on invalid input, so surface the validator's message
    // ourselves. Gate on non-empty input so the dialog does not greet the operator with a
    // "may not be empty" error before they have typed anything.
    let guard = form_ctx.read();
    let error = match guard.get_field_valid("keys") {
        Some(Err(msg)) if !guard.get_field_text("keys").trim().is_empty() => Some(msg),
        _ => None,
    };
    drop(guard);

    // The textarea opts into `width: 100%` so it fills the InputPanel's grid cell instead of
    // shrinking to browser-default cols.
    let mut panel = InputPanel::new()
        .padding(4)
        .min_width(500)
        .with_large_custom_child(
            TextArea::new()
                .name("keys")
                .submit_empty(false)
                .required(true)
                // Validate each pasted key against the shared schema so a typo lands inline here
                // instead of as a server-side rejection after submit. Split exactly like
                // `submit_add_keys` so the field and the request agree on token boundaries, and
                // report every offending token at once so a bulk paste does not turn into a
                // fix-one-resubmit loop.
                .validate(|value: &String| {
                    let mut found = false;
                    let mut bad: Vec<&str> = Vec::new();
                    for token in value.split(|c: char| c.is_whitespace() || c == ',') {
                        let token = token.trim();
                        if token.is_empty() {
                            continue;
                        }
                        found = true;
                        if SUBSCRIPTION_KEY_SCHEMA.parse_simple_value(token).is_err() {
                            bad.push(token);
                        }
                    }
                    if !found {
                        anyhow::bail!(tr!("no keys provided"));
                    }
                    if !bad.is_empty() {
                        let shown = bad.iter().take(5).copied().collect::<Vec<_>>().join(", ");
                        let extra = bad.len().saturating_sub(5);
                        let msg = if extra == 0 {
                            tr!(
                                "not a valid Proxmox VE or Proxmox Backup Server key: {0}"
                                    | "not valid Proxmox VE or Proxmox Backup Server keys: {0}"
                                    % bad.len() as u64,
                                shown
                            )
                        } else {
                            tr!(
                                "not valid Proxmox VE or Proxmox Backup Server keys: {0} (and {1} more)",
                                shown,
                                extra
                            )
                        };
                        anyhow::bail!(msg);
                    }
                    Ok(())
                })
                .attribute("rows", "8")
                .attribute("placeholder", tr!("Subscription key(s)"))
                .style("width", "100%")
                .style("box-sizing", "border-box"),
        )
        .with_large_custom_child(hint);

    if let Some(msg) = error {
        panel = panel.with_large_custom_child(
            Container::new()
                .class(FontStyle::TitleSmall)
                .class(FontColor::Error)
                .padding_top(1)
                .with_child(msg),
        );
    }

    panel.into()
}

async fn submit_add_keys(form_ctx: FormContext, digest: Option<String>) -> Result<(), Error> {
    let raw = form_ctx.read().get_field_text("keys");
    // Dedup before submit so a pasted-twice key is a no-op, not a server rejection.
    let mut seen = std::collections::HashSet::new();
    let keys: Vec<String> = raw
        .split(|c: char| c.is_whitespace() || c == ',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|s| seen.insert(s.to_string()))
        .map(str::to_string)
        .collect();

    if keys.is_empty() {
        anyhow::bail!(tr!("no keys provided"));
    }

    let mut body = serde_json::json!({ "keys": keys });
    if let Some(d) = digest {
        body["digest"] = d.into();
    }
    http_post::<AddKeysResult>(BASE_URL, Some(body)).await?;
    Ok(())
}

/// Map a subscription product type to the remote type its keys can drive.
fn remote_type_for(product_type: ProductType) -> Option<RemoteType> {
    if product_type.matches_remote_type(RemoteType::Pve) {
        Some(RemoteType::Pve)
    } else if product_type.matches_remote_type(RemoteType::Pbs) {
        Some(RemoteType::Pbs)
    } else {
        None
    }
}

fn assign_input_panel(
    key: &str,
    product_type: ProductType,
    form_ctx: &FormContext,
    node_status: &[RemoteNodeStatus],
) -> Html {
    let mut panel = InputPanel::new().padding(4).min_width(500).with_field(
        tr!("Key"),
        DisplayField::new()
            .name("key")
            .value(key.to_string())
            .key("key-display"),
    );

    let Some(remote_type) = remote_type_for(product_type) else {
        // Defensive: the toolbar disables Assign for these product types.
        return panel
            .with_large_custom_child(
                Container::new()
                    .class(FontStyle::TitleSmall)
                    .class(Opacity::Quarter)
                    .with_child(tr!(
                        "PDM cannot manage {product} remotes yet; this key is parked in the pool.",
                        product = product_type.to_string(),
                    )),
            )
            .into();
    };

    // Hide remotes of this type whose every node already has a pool key assigned; they offer no
    // free target for this key.
    let excluded_remotes: Rc<Vec<AttrValue>> = {
        use std::collections::BTreeSet;
        let mut all_of_type: BTreeSet<&str> = BTreeSet::new();
        let mut assignable: BTreeSet<&str> = BTreeSet::new();
        for n in node_status.iter().filter(|n| n.ty == remote_type) {
            all_of_type.insert(n.remote.as_str());
            if n.assigned_key.is_none() {
                assignable.insert(n.remote.as_str());
            }
        }
        Rc::new(
            all_of_type
                .difference(&assignable)
                .map(|r| AttrValue::from(r.to_string()))
                .collect(),
        )
    };

    panel = panel.with_field(
        tr!("Remote"),
        RemoteSelector::new()
            .name("remote")
            .remote_type(remote_type)
            .excluded_remotes(excluded_remotes)
            .required(true),
    );

    match remote_type {
        RemoteType::Pve => {
            let selected_remote = form_ctx.read().get_field_text("remote");
            if selected_remote.is_empty() {
                panel
                    .with_field(
                        tr!("Node"),
                        DisplayField::new()
                            .name("node")
                            .key("node-no-remote")
                            .value(AttrValue::from(tr!("Select a remote first."))),
                    )
                    .into()
            } else {
                let excluded: Vec<String> = node_status
                    .iter()
                    .filter(|n| n.remote == selected_remote && n.assigned_key.is_some())
                    .map(|n| n.node.clone())
                    .collect();
                // `PveNodeSelector` fetches its node list in `create` and does not re-fetch on
                // prop change, so a per-remote `key` forces a fresh component when the operator
                // picks a target.
                panel
                    .with_field(
                        tr!("Node"),
                        PveNodeSelector::new(selected_remote.clone())
                            .name("node")
                            .key(format!("node-selector-{selected_remote}"))
                            .excluded_nodes(Rc::new(excluded))
                            .show_memory(false)
                            .required(true),
                    )
                    .into()
            }
        }
        RemoteType::Pbs => panel
            .with_field(
                tr!("Node"),
                DisplayField::new()
                    .name("node")
                    .value(AttrValue::from("localhost"))
                    .key("node-localhost"),
            )
            .into(),
    }
}

async fn submit_assign(
    key: String,
    form_ctx: FormContext,
    digest: Option<String>,
) -> Result<(), Error> {
    let mut data = form_ctx.get_submit_data();
    if let Some(d) = digest {
        if let Some(obj) = data.as_object_mut() {
            obj.insert("digest".to_string(), d.into());
        }
    }
    let url = format!("{BASE_URL}/{}/assignment", percent_encode_component(&key));
    http_post(&url, Some(data)).await
}
