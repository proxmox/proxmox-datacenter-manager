//! Node-first Assign Key dialog opened from the Subscription Registry's node tree panel.

use std::rc::Rc;

use anyhow::Error;
use serde_json::json;

use yew::html::IntoEventCallback;
use yew::virtual_dom::{Key, VComp, VNode};

use pwt::css::{FlexFit, FontColor, JustifyContent};
use pwt::prelude::*;
use pwt::props::{ContainerBuilder, WidgetBuilder};
use pwt::state::{Selection, Store};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::{Button, Column, Container, Dialog, Mask, Row};

use proxmox_yew_comp::http_post;
use proxmox_yew_comp::percent_encoding::percent_encode_component;

use pdm_api_types::remotes::RemoteType;
use pdm_api_types::subscription::{
    SubscriptionKeyEntry, pick_best_pve_socket_key, socket_count_from_key,
};

const KEYS_URL: &str = "/subscriptions/keys";

/// The node a key is about to be assigned to. Bundles the four target attributes so they travel
/// together instead of as loose positional arguments where a `String` could be swapped for
/// another.
#[derive(Clone, PartialEq)]
pub struct AssignTarget {
    pub remote: String,
    pub node: String,
    pub ty: RemoteType,
    pub sockets: Option<i64>,
}

/// Filter the pool to keys that can land on a `remote_type` node and are not yet bound.
fn candidates_for(
    pool_keys: &[SubscriptionKeyEntry],
    remote_type: RemoteType,
) -> Vec<SubscriptionKeyEntry> {
    let mut out: Vec<SubscriptionKeyEntry> = pool_keys
        .iter()
        .filter(|e| e.remote.is_none() && e.product_type.matches_remote_type(remote_type))
        .cloned()
        .collect();
    // PVE: smallest covering socket count first so the default selection is the cheapest fit
    // that still works. PBS keys have no socket count, fall back to key string.
    out.sort_by(|a, b| {
        let sa = socket_count_from_key(&a.key);
        let sb = socket_count_from_key(&b.key);
        sa.cmp(&sb).then_with(|| a.key.cmp(&b.key))
    });
    out
}

/// Pick a sensible default key for the dialog. For PVE, the smallest covering socket-count;
/// for PBS, the first candidate.
fn default_candidate(
    candidates: &[SubscriptionKeyEntry],
    remote_type: RemoteType,
    node_sockets: Option<i64>,
) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }
    if remote_type == RemoteType::Pve {
        let needed = node_sockets.unwrap_or(1).max(1) as u32;
        if let Some(picked) = pick_best_pve_socket_key(
            needed,
            candidates.iter().map(|e| (e.key.clone(), e.key.as_str())),
        ) {
            return Some(picked);
        }
    }
    candidates.first().map(|e| e.key.clone())
}

fn key_columns() -> Rc<Vec<DataTableHeader<SubscriptionKeyEntry>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Key"))
            .flex(2)
            .get_property(|e: &SubscriptionKeyEntry| e.key.as_str())
            .into(),
        DataTableColumn::new(tr!("Product"))
            .width("80px")
            .render(|e: &SubscriptionKeyEntry| e.product_type.to_string().into())
            .into(),
        DataTableColumn::new(tr!("Level"))
            .width("100px")
            .render(|e: &SubscriptionKeyEntry| e.level.to_string().into())
            .into(),
        DataTableColumn::new(tr!("Sockets"))
            .width("80px")
            .render(|e: &SubscriptionKeyEntry| {
                socket_count_from_key(&e.key)
                    .map(|s| s.to_string())
                    .unwrap_or_default()
                    .into()
            })
            .into(),
    ])
}

async fn submit_assignment(
    key: &str,
    remote: &str,
    node: &str,
    digest: Option<&str>,
) -> Result<(), Error> {
    let url = format!("{KEYS_URL}/{}/assignment", percent_encode_component(key));
    let mut body = json!({ "remote": remote, "node": node });
    if let Some(d) = digest {
        body["digest"] = d.into();
    }
    http_post::<()>(&url, Some(body)).await
}

/// Simple "Assign Key to <remote>/<node>" dialog.
#[derive(Properties, Clone, PartialEq)]
pub struct AssignKeyToNodeDialog {
    pub target: AssignTarget,
    pub pool_keys: Rc<Vec<SubscriptionKeyEntry>>,

    #[prop_or_default]
    pub pool_digest: Option<String>,

    #[prop_or_default]
    pub on_done: Option<Callback<()>>,
}

impl AssignKeyToNodeDialog {
    pub fn new(target: AssignTarget, pool_keys: Rc<Vec<SubscriptionKeyEntry>>) -> Self {
        Self {
            target,
            pool_keys,
            pool_digest: None,
            on_done: None,
        }
    }

    pub fn pool_digest(mut self, digest: Option<String>) -> Self {
        self.pool_digest = digest;
        self
    }

    pub fn on_done(mut self, cb: impl IntoEventCallback<()>) -> Self {
        self.on_done = cb.into_event_callback();
        self
    }
}

impl From<AssignKeyToNodeDialog> for VNode {
    fn from(val: AssignKeyToNodeDialog) -> Self {
        VComp::new::<AssignKeyToNodeComp>(Rc::new(val), None).into()
    }
}

pub enum AssignMsg {
    SelectionChanged,
    Submit,
    SubmitDone(Result<(), Error>),
}

pub struct AssignKeyToNodeComp {
    store: Store<SubscriptionKeyEntry>,
    columns: Rc<Vec<DataTableHeader<SubscriptionKeyEntry>>>,
    selection: Selection,
    last_error: Option<String>,
    submitting: bool,
}

impl yew::Component for AssignKeyToNodeComp {
    type Message = AssignMsg;
    type Properties = AssignKeyToNodeDialog;

    fn create(ctx: &yew::Context<Self>) -> Self {
        let props = ctx.props();
        let candidates = candidates_for(&props.pool_keys, props.target.ty);
        let default = default_candidate(&candidates, props.target.ty, props.target.sockets);

        let store = Store::with_extract_key(|e: &SubscriptionKeyEntry| Key::from(e.key.as_str()));
        store.set_data(candidates);

        let selection = Selection::new().on_select({
            let link = ctx.link().clone();
            move |_| link.send_message(AssignMsg::SelectionChanged)
        });
        if let Some(key) = default {
            selection.select(Key::from(key));
        }

        Self {
            store,
            columns: key_columns(),
            selection,
            last_error: None,
            submitting: false,
        }
    }

    fn update(&mut self, ctx: &yew::Context<Self>, msg: Self::Message) -> bool {
        match msg {
            AssignMsg::SelectionChanged => true,
            AssignMsg::Submit => {
                let Some(picked) = self.selection.selected_key() else {
                    self.last_error = Some(tr!("Select a key first."));
                    return true;
                };
                let key = picked.to_string();
                let remote = ctx.props().target.remote.clone();
                let node = ctx.props().target.node.clone();
                let digest = ctx.props().pool_digest.clone();
                self.submitting = true;
                self.last_error = None;
                ctx.link().send_future(async move {
                    let res = submit_assignment(&key, &remote, &node, digest.as_deref()).await;
                    AssignMsg::SubmitDone(res)
                });
                true
            }
            AssignMsg::SubmitDone(Ok(())) => {
                self.submitting = false;
                if let Some(cb) = &ctx.props().on_done {
                    cb.emit(());
                }
                false
            }
            AssignMsg::SubmitDone(Err(err)) => {
                self.submitting = false;
                self.last_error = Some(err.to_string());
                true
            }
        }
    }

    fn view(&self, ctx: &yew::Context<Self>) -> Html {
        let props = ctx.props();
        let no_candidates = self.store.read().len() == 0;

        // The dialog title already carries `{remote}/{node}`; render only the sockets line here
        // so the body adds context the title cannot fit. Without sockets there is nothing to add.
        let header: Option<Html> = props.target.sockets.map(|s| {
            Row::new()
                .gap(2)
                .with_child(Container::new().with_child(tr!("Node sockets:")))
                .with_child(Container::new().with_child(s.to_string()))
                .into()
        });

        let body_keys: Html = if no_candidates {
            Container::new()
                .padding(2)
                .with_child(tr!(
                    "No matching free keys in the pool. Add one via the Key Pool panel first."
                ))
                .into()
        } else {
            DataTable::new(self.columns.clone(), self.store.clone())
                .selection(self.selection.clone())
                .striped(true)
                .min_height(140)
                .class(FlexFit)
                .into()
        };

        let mut footer = Row::new()
            .gap(2)
            .class(JustifyContent::FlexEnd)
            .with_flex_spacer()
            .with_child(Button::new(tr!("Cancel")).on_activate({
                let cb = props.on_done.clone();
                move |_| {
                    if let Some(cb) = &cb {
                        cb.emit(());
                    }
                }
            }))
            .with_child(
                Button::new(tr!("Assign"))
                    .disabled(no_candidates || self.submitting)
                    .on_activate(ctx.link().callback(|_| AssignMsg::Submit)),
            );

        if let Some(err) = &self.last_error {
            footer = footer.with_child(
                Container::new()
                    .padding_x(2)
                    .class(FontColor::Error)
                    .with_child(err.clone()),
            );
        }

        // FlexFit keeps the footer flush on resize; size constraints live on the Dialog so the
        // body never out-grows it.
        let mut body = Column::new().class(FlexFit).padding(2).gap(2).min_height(0);
        if let Some(h) = header {
            body = body.with_child(h);
        }
        let body = body.with_child(body_keys).with_child(footer);

        Dialog::new(tr!(
            "Assign Key to {remote}/{node}",
            remote = props.target.remote.clone(),
            node = props.target.node.clone()
        ))
        .resizable(true)
        .min_width(640)
        .min_height(300)
        .max_height("80vh")
        .on_close({
            let cb = props.on_done.clone();
            move |_| {
                if let Some(cb) = &cb {
                    cb.emit(());
                }
            }
        })
        .with_child(
            Mask::new(body)
                .visible(self.submitting)
                .text(tr!("Assigning...")),
        )
        .into()
    }
}
