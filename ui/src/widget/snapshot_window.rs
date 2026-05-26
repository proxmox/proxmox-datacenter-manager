//! Manage the snapshots of a single PVE guest (QEMU or LXC).
//!
//! Rendered either inline as a guest-detail tab ([`SnapshotWindow::embedded`]) or as a modal
//! dialog. Both share the same [`LoadableComponent`] implementation; only the outer wrapping
//! differs (see [`From<SnapshotWindow> for VNode`]).

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::{bail, Error};
use serde_json::json;
use yew::html::IntoEventCallback;
use yew::virtual_dom::{Key, VComp, VNode};
use yew::{Callback, Properties};

use proxmox_client::ApiResponseData;
use proxmox_yew_comp::{
    utils::render_epoch, EditWindow, LoadableComponent, LoadableComponentContext,
    LoadableComponentMaster, LoadableComponentScope, LoadableComponentScopeExt,
    LoadableComponentState,
};
use pwt::css::FlexFit;
use pwt::prelude::*;
use pwt::props::ExtractPrimaryKey;
use pwt::state::{KeyedSlabTree, TreeStore};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::{Checkbox, Field, FormContext};
use pwt::widget::{
    ActionIcon, Button, Container, Dialog, Fa, InputPanel, MessageBox, MessageBoxButtons, Row,
    Toolbar, Tooltip,
};
use pwt_macros::builder;

use pdm_api_types::RemoteUpid;

use crate::pve::{GuestInfo, GuestType};

/// A guest snapshot, unified across QEMU and LXC (LXC has no `vmstate`).
#[derive(Clone, PartialEq)]
struct SnapshotItem {
    name: String,
    description: String,
    snaptime: Option<i64>,
    /// RAM included; only ever `Some` for QEMU.
    vmstate: Option<bool>,
    /// Name of the snapshot this one descends from, if any.
    parent: Option<String>,
}

impl SnapshotItem {
    fn is_current(&self) -> bool {
        self.name == "current"
    }
}

/// Tree node: snapshots form a parent/child tree via their `parent` link, mirroring PVE.
#[derive(Clone, PartialEq)]
enum SnapshotTreeEntry {
    Root,
    Item(SnapshotItem),
}

impl ExtractPrimaryKey for SnapshotTreeEntry {
    fn extract_key(&self) -> Key {
        match self {
            // Snapshot names must start with a letter (pve-configid), so this synthetic key
            // can never collide with an item key (a duplicate insert would panic in debug).
            SnapshotTreeEntry::Root => Key::from("__root__"),
            SnapshotTreeEntry::Item(s) => Key::from(s.name.clone()),
        }
    }
}

/// Build the snapshot tree from `parent` links. Each snapshot nests under its parent; snapshots
/// with no (or an unknown) parent sit at the top. The synthetic `current` state attaches under the
/// snapshot it descends from. Built iteratively (place a node once its parent exists) so a
/// malformed/cyclic parent chain cannot recurse forever - leftovers are attached at the top.
///
/// All nodes are force-expanded on every build: the tree is small and this keeps the current
/// state (NOW) and a just-created snapshot visible after each create/delete/rollback reload.
fn build_snapshot_tree(mut items: Vec<SnapshotItem>) -> KeyedSlabTree<SnapshotTreeEntry> {
    let names: HashSet<String> = items.iter().map(|s| s.name.clone()).collect();
    // Stable sibling order by time; the parentless 'current' (snaptime None) sorts last.
    items.sort_by_key(|s| (s.is_current(), s.snaptime.unwrap_or(i64::MAX)));

    let root_key = Key::from("__root__");
    let mut tree = KeyedSlabTree::new();
    tree.set_root(SnapshotTreeEntry::Root).set_expanded(true);

    let mut placed: HashSet<String> = HashSet::new();
    let mut remaining = items;
    loop {
        let mut progressed = false;
        let mut next = Vec::new();
        for item in remaining {
            let parent = match &item.parent {
                Some(p) if names.contains(p) => Some(p.clone()),
                _ => None,
            };
            let ready = parent.as_ref().map(|p| placed.contains(p)).unwrap_or(true);
            if !ready {
                next.push(item);
                continue;
            }
            let parent_key = parent.map(Key::from).unwrap_or_else(|| root_key.clone());
            if let Some(mut parent_node) = tree.lookup_node_mut(&parent_key) {
                placed.insert(item.name.clone());
                parent_node
                    .append(SnapshotTreeEntry::Item(item))
                    .set_expanded(true);
                progressed = true;
            }
        }
        remaining = next;
        if remaining.is_empty() {
            break;
        }
        if !progressed {
            // Orphans or a parent cycle: attach what's left at the top so nothing is dropped.
            if let Some(mut root) = tree.lookup_node_mut(&root_key) {
                for item in remaining {
                    root.append(SnapshotTreeEntry::Item(item))
                        .set_expanded(true);
                }
            }
            break;
        }
    }
    tree
}

#[derive(PartialEq, Properties)]
#[builder]
pub struct SnapshotWindow {
    /// The remote the guest lives on.
    pub remote: AttrValue,

    /// Which guest to manage snapshots for.
    pub guest_info: GuestInfo,

    /// Render the list inline (for embedding as a tab) instead of as a modal dialog.
    #[prop_or_default]
    #[builder]
    pub embedded: bool,

    /// Close callback (only used in modal mode).
    #[prop_or_default]
    #[builder_cb(IntoEventCallback, into_event_callback, ())]
    pub on_close: Option<Callback<()>>,
}

impl SnapshotWindow {
    pub fn new(remote: impl Into<AttrValue>, guest_info: GuestInfo) -> Self {
        yew::props!(Self {
            remote: remote.into(),
            guest_info,
        })
    }
}

impl From<SnapshotWindow> for VNode {
    fn from(val: SnapshotWindow) -> Self {
        let embedded = val.embedded;
        let on_close = val.on_close.clone();
        let title = tr!("Snapshots - {0}", val.guest_info.vmid);

        // The master renders a Column/page (toolbar + table + dialogs), not a dialog. Embedded as
        // a detail-panel tab it is dropped in inline; as a standalone window it is wrapped in a
        // Dialog that carries the title and the close callback.
        let master: VNode =
            VComp::new::<LoadableComponentMaster<PdmSnapshotWindow>>(Rc::new(val), None).into();
        if embedded {
            return master;
        }

        Dialog::new(title)
            .min_width(720)
            .min_height(440)
            .max_height("90vh")
            .resizable(true)
            .on_close(on_close)
            .with_child(master)
            .into()
    }
}

/// The active sub-dialog, rendered by [`LoadableComponent::dialog_view`] over the snapshot list.
#[derive(PartialEq)]
enum ViewState {
    /// Take a new snapshot.
    Create,
    /// Edit a snapshot's description: (snapshot name, current description for the dirty baseline).
    EditDescription(String, String),
    /// Confirm deleting a snapshot.
    ConfirmDelete(String),
    /// Confirm rolling back to a snapshot (offers the start-after-rollback option).
    ConfirmRollback(String),
}

enum Msg {
    /// Stash a finished load into the tree store.
    LoadResult(Vec<SnapshotItem>),
    /// A snapshot action started a task; show its auto-closing progress.
    ShowTask(RemoteUpid),
    /// Run an action (delete or rollback) against a snapshot, on confirmation.
    RunAction {
        snapname: String,
        rollback: bool,
        start: bool,
    },
    /// Set a snapshot's description (PVE endpoint is synchronous, no task/UPID).
    UpdateDescription(String, String),
}

#[doc(hidden)]
struct PdmSnapshotWindow {
    state: LoadableComponentState<ViewState>,
    store: TreeStore<SnapshotTreeEntry>,
    columns: Rc<Vec<DataTableHeader<SnapshotTreeEntry>>>,
}

pwt::impl_deref_mut_property!(PdmSnapshotWindow, state, LoadableComponentState<ViewState>);

impl PdmSnapshotWindow {
    /// Derive the task base URL from the UPID's own remote, then show the auto-closing task
    /// progress (it dismisses on success and reloads the list).
    fn show_task(&mut self, ctx: &LoadableComponentContext<Self>, upid: RemoteUpid) {
        self.set_task_base_url(
            format!("/{}/remotes/{}/tasks", upid.remote_type(), upid.remote()).into(),
        );
        ctx.link().show_task_progres(upid.to_string());
    }

    fn create_input_panel(guest_type: GuestType) -> InputPanel {
        let mut panel = InputPanel::new()
            .padding(4)
            .with_field(
                tr!("Name"),
                Field::new()
                    .name("snapname")
                    .required(true)
                    .autofocus(true)
                    // mirror PVE's pve-configid so invalid names are caught before submit;
                    // the server still validates authoritatively.
                    .validate(|name: &String| {
                        if name.is_empty() {
                            return Ok(()); // 'required' reports the empty case
                        }
                        if name.len() > 40 {
                            bail!(tr!("Name must be at most 40 characters long."));
                        }
                        let starts_with_letter =
                            name.starts_with(|c: char| c.is_ascii_alphabetic());
                        let valid_chars = name
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
                        if !starts_with_letter || !valid_chars {
                            bail!(tr!(
                                "Use only letters, digits, '_' and '-', starting with a letter."
                            ));
                        }
                        Ok(())
                    }),
            )
            .with_field(tr!("Description"), Field::new().name("description"));
        if guest_type == GuestType::Qemu {
            panel.add_field(
                tr!("Include RAM"),
                Checkbox::new().name("vmstate").submit(true),
            );
        }
        panel
    }

    async fn submit_create(
        remote: AttrValue,
        guest_info: GuestInfo,
        form_ctx: FormContext,
    ) -> Result<RemoteUpid, Error> {
        let data = form_ctx.get_submit_data();
        let snapname = data["snapname"].as_str().unwrap_or_default().to_string();
        let description = data["description"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let client = crate::pdm_client();
        // node is resolved server-side via find_node_for_vm
        let node = None;
        let upid = match guest_info.guest_type {
            GuestType::Qemu => {
                let vmstate = data["vmstate"].as_bool();
                client
                    .pve_qemu_snapshot_create(
                        &remote,
                        node,
                        guest_info.vmid,
                        &snapname,
                        description.as_deref(),
                        vmstate,
                    )
                    .await?
            }
            GuestType::Lxc => {
                client
                    .pve_lxc_snapshot_create(
                        &remote,
                        node,
                        guest_info.vmid,
                        &snapname,
                        description.as_deref(),
                    )
                    .await?
            }
        };
        Ok(upid)
    }

    async fn run_action(
        remote: AttrValue,
        guest_info: GuestInfo,
        snapname: String,
        rollback: bool,
        start: bool,
    ) -> Result<RemoteUpid, Error> {
        let client = crate::pdm_client();
        let node = None;
        // only forward `start` when set, so the server default is otherwise unchanged
        let start = start.then_some(true);
        let upid = match (guest_info.guest_type, rollback) {
            (GuestType::Qemu, false) => {
                client
                    .pve_qemu_snapshot_delete(&remote, node, guest_info.vmid, &snapname)
                    .await?
            }
            (GuestType::Qemu, true) => {
                client
                    .pve_qemu_snapshot_rollback(&remote, node, guest_info.vmid, &snapname, start)
                    .await?
            }
            (GuestType::Lxc, false) => {
                client
                    .pve_lxc_snapshot_delete(&remote, node, guest_info.vmid, &snapname)
                    .await?
            }
            (GuestType::Lxc, true) => {
                client
                    .pve_lxc_snapshot_rollback(&remote, node, guest_info.vmid, &snapname, start)
                    .await?
            }
        };
        Ok(upid)
    }

    async fn update_description(
        remote: AttrValue,
        guest_info: GuestInfo,
        snapname: String,
        description: String,
    ) -> Result<(), Error> {
        let client = crate::pdm_client();
        let node = None;
        match guest_info.guest_type {
            GuestType::Qemu => {
                client
                    .pve_qemu_snapshot_update_config(
                        &remote,
                        node,
                        guest_info.vmid,
                        &snapname,
                        Some(&description),
                    )
                    .await?
            }
            GuestType::Lxc => {
                client
                    .pve_lxc_snapshot_update_config(
                        &remote,
                        node,
                        guest_info.vmid,
                        &snapname,
                        Some(&description),
                    )
                    .await?
            }
        }
        Ok(())
    }
}

impl LoadableComponent for PdmSnapshotWindow {
    type Properties = SnapshotWindow;
    type Message = Msg;
    type ViewState = ViewState;

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        // root stays hidden so the snapshots are the top-level rows
        let store = TreeStore::new().view_root(false);
        Self {
            state: LoadableComponentState::new(),
            columns: columns(ctx.link().clone(), &ctx.props().guest_info, store.clone()),
            store,
        }
    }

    fn load(
        &self,
        ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        let props = ctx.props();
        let remote = props.remote.clone();
        let guest_info = props.guest_info;
        let link = ctx.link().clone();
        Box::pin(async move {
            let client = crate::pdm_client();
            // node is resolved server-side via find_node_for_vm
            let node = None;
            let items: Vec<SnapshotItem> = match guest_info.guest_type {
                GuestType::Qemu => client
                    .pve_qemu_list_snapshots(&remote, node, guest_info.vmid)
                    .await?
                    .into_iter()
                    .map(|s| SnapshotItem {
                        name: s.name,
                        description: s.description,
                        snaptime: s.snaptime,
                        vmstate: s.vmstate,
                        parent: s.parent,
                    })
                    .collect(),
                GuestType::Lxc => client
                    .pve_lxc_list_snapshots(&remote, node, guest_info.vmid)
                    .await?
                    .into_iter()
                    .map(|s| SnapshotItem {
                        name: s.name,
                        description: s.description,
                        snaptime: s.snaptime,
                        vmstate: None,
                        parent: s.parent,
                    })
                    .collect(),
            };
            link.send_message(Msg::LoadResult(items));
            Ok(())
        })
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadResult(items) => {
                self.store
                    .write()
                    .update_root_tree(build_snapshot_tree(items));
            }
            Msg::ShowTask(upid) => self.show_task(ctx, upid),
            Msg::RunAction {
                snapname,
                rollback,
                start,
            } => {
                let props = ctx.props();
                let remote = props.remote.clone();
                let guest_info = props.guest_info;
                let link = ctx.link().clone();
                ctx.link().spawn(async move {
                    match Self::run_action(remote, guest_info, snapname, rollback, start).await {
                        Ok(upid) => link.send_message(Msg::ShowTask(upid)),
                        Err(err) => link.show_error(tr!("Error"), err, true),
                    }
                });
            }
            Msg::UpdateDescription(snapname, description) => {
                let props = ctx.props();
                let remote = props.remote.clone();
                let guest_info = props.guest_info;
                let link = ctx.link().clone();
                ctx.link().spawn(async move {
                    match Self::update_description(remote, guest_info, snapname, description).await
                    {
                        Ok(()) => {
                            link.send_reload();
                            link.change_view(None);
                        }
                        Err(err) => link.show_error(tr!("Error"), err, true),
                    }
                });
            }
        }
        true
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        let link = ctx.link();
        Some(
            Toolbar::new()
                .border_bottom(true)
                .with_child(
                    Button::new(tr!("Take Snapshot"))
                        .icon_class("fa fa-camera")
                        .on_activate(link.change_view_callback(|_| Some(ViewState::Create))),
                )
                .with_flex_spacer()
                .with_child(Button::refresh(self.loading()).on_activate({
                    let link = link.clone();
                    move |_| link.send_reload()
                }))
                .into(),
        )
    }

    fn main_view(&self, _ctx: &LoadableComponentContext<Self>) -> Html {
        DataTable::new(Rc::clone(&self.columns), self.store.clone())
            .class(FlexFit)
            .into()
    }

    fn dialog_view(
        &self,
        ctx: &LoadableComponentContext<Self>,
        view_state: &Self::ViewState,
    ) -> Option<Html> {
        let props = ctx.props();
        let link = ctx.link();
        match view_state {
            ViewState::Create => {
                let remote = props.remote.clone();
                let guest_info = props.guest_info;
                let guest_type = guest_info.guest_type;
                Some(
                    EditWindow::new(tr!("Take Snapshot"))
                        .edit(false)
                        .submit_text(tr!("Take Snapshot"))
                        .on_close(link.change_view_callback(|_| None))
                        .renderer(move |_form_ctx: &FormContext| {
                            Self::create_input_panel(guest_type).into()
                        })
                        .on_submit({
                            let link = link.clone();
                            move |form_ctx: FormContext| {
                                let link = link.clone();
                                let remote = remote.clone();
                                async move {
                                    let upid =
                                        Self::submit_create(remote, guest_info, form_ctx).await?;
                                    link.send_message(Msg::ShowTask(upid));
                                    Ok(())
                                }
                            }
                        })
                        .into(),
                )
            }
            ViewState::EditDescription(name, description) => {
                let name = name.clone();
                Some(
                    EditWindow::new(tr!("Edit Description"))
                        .edit(true)
                        .submit_text(tr!("OK"))
                        .on_close(link.change_view_callback(|_| None))
                        // prefill the form and set the dirty baseline to the current description
                        .loader({
                            let description = description.clone();
                            move || {
                                let description = description.clone();
                                async move {
                                    Ok(ApiResponseData {
                                        attribs: Default::default(),
                                        data: json!({ "description": description }),
                                    })
                                }
                            }
                        })
                        .renderer(move |_form: &FormContext| {
                            InputPanel::new()
                                .padding(4)
                                .with_field(tr!("Description"), Field::new().name("description"))
                                .into()
                        })
                        .on_submit({
                            let link = link.clone();
                            move |form_ctx: FormContext| {
                                let description = form_ctx.read().get_field_text("description");
                                let link = link.clone();
                                let name = name.clone();
                                async move {
                                    link.send_message(Msg::UpdateDescription(name, description));
                                    Ok(())
                                }
                            }
                        })
                        .into(),
                )
            }
            ViewState::ConfirmDelete(name) => {
                let name = name.clone();
                Some(
                    MessageBox::new(
                        tr!("Delete Snapshot"),
                        tr!("Are you sure you want to delete snapshot '{0}'?", name),
                    )
                    .buttons(MessageBoxButtons::YesNo)
                    .on_close({
                        let link = link.clone();
                        move |confirm| {
                            if confirm {
                                link.send_message(Msg::RunAction {
                                    snapname: name.clone(),
                                    rollback: false,
                                    start: false,
                                });
                            }
                            link.change_view(None);
                        }
                    })
                    .into(),
                )
            }
            ViewState::ConfirmRollback(name) => {
                // EditWindow (not MessageBox) so we can offer the "start after rollback" option.
                let name = name.clone();
                Some(
                    EditWindow::new(tr!("Rollback Snapshot"))
                        .edit(false)
                        .submit_text(tr!("Rollback"))
                        .on_close(link.change_view_callback(|_| None))
                        .renderer({
                            let name = name.clone();
                            move |_form: &FormContext| {
                                InputPanel::new()
                                    .padding(4)
                                    .with_large_custom_child(
                                        Container::new().key("warn").padding_bottom(2).with_child(
                                            tr!(
                                                "Roll back to snapshot '{0}'? This reverts the \
                                                 guest's disk and configuration to that snapshot; \
                                                 changes made since are lost. A running guest is \
                                                 stopped during rollback (unless a QEMU snapshot \
                                                 includes its memory state).",
                                                name
                                            ),
                                        ),
                                    )
                                    .with_field(
                                        tr!("Start After Rollback"),
                                        Checkbox::new().name("start").submit(true),
                                    )
                                    .into()
                            }
                        })
                        .on_submit({
                            let link = link.clone();
                            move |form_ctx: FormContext| {
                                let start = form_ctx.read().get_field_checked("start");
                                let link = link.clone();
                                let name = name.clone();
                                async move {
                                    link.send_message(Msg::RunAction {
                                        snapname: name,
                                        rollback: true,
                                        start,
                                    });
                                    Ok(())
                                }
                            }
                        })
                        .into(),
                )
            }
        }
    }
}

fn columns(
    link: LoadableComponentScope<PdmSnapshotWindow>,
    guest_info: &GuestInfo,
    store: TreeStore<SnapshotTreeEntry>,
) -> Rc<Vec<DataTableHeader<SnapshotTreeEntry>>> {
    // The Name and Date columns intentionally have no sorter: ordering is structural (each
    // snapshot nests under its parent, siblings sorted by snaptime in build_snapshot_tree).
    let mut cols: Vec<DataTableHeader<SnapshotTreeEntry>> = vec![
        DataTableColumn::new(tr!("Name"))
            .flex(1)
            .tree_column(store)
            .render(|e: &SnapshotTreeEntry| {
                let SnapshotTreeEntry::Item(s) = e else {
                    return html! {};
                };
                if s.is_current() {
                    html! { <b>{ tr!("NOW") }</b> }
                } else {
                    html! { { s.name.clone() } }
                }
            })
            .into(),
        DataTableColumn::new(tr!("Date"))
            .width("160px")
            .render(|e: &SnapshotTreeEntry| match e {
                SnapshotTreeEntry::Item(s) => match s.snaptime {
                    Some(t) => render_epoch(t).into(),
                    None => html! { {"-"} },
                },
                SnapshotTreeEntry::Root => html! {},
            })
            .into(),
    ];

    // RAM/memory state only exists for QEMU; the column would always be blank for LXC.
    if guest_info.guest_type == GuestType::Qemu {
        cols.push(
            DataTableColumn::new(tr!("RAM"))
                .width("60px")
                .justify("center")
                .render(|e: &SnapshotTreeEntry| match e {
                    SnapshotTreeEntry::Item(s) if s.vmstate == Some(true) => {
                        Fa::new("check").into()
                    }
                    _ => html! {},
                })
                .into(),
        );
    }

    // Actions sit before Description (the wide flex column) to minimize horizontal mouse travel
    // from the row's identity to its controls, especially on wide desktops.
    cols.push(
        DataTableColumn::new(tr!("Actions"))
            .width("120px")
            .justify("center")
            .render(move |e: &SnapshotTreeEntry| {
                let SnapshotTreeEntry::Item(s) = e else {
                    return html! {};
                };
                // 'current'/NOW is synthetic: nothing to edit, roll back to, or delete.
                if s.is_current() {
                    return html! {};
                }
                let name = s.name.clone();
                let description = s.description.clone();
                // ActionIcons default to tabindex -1; set 0 + aria-label so each is reachable and
                // named for keyboard/screen-reader users (the Tooltip is not an accessible name).
                // Order edit -> rollback -> delete: least to most destructive, left to right.
                Row::new()
                    .gap(2)
                    .class(pwt::css::JustifyContent::Center)
                    .with_child(
                        Tooltip::new(
                            ActionIcon::new("fa fa-fw fa-pencil")
                                .tabindex(0)
                                .aria_label(tr!("Edit Description"))
                                .on_activate({
                                    let link = link.clone();
                                    let name = name.clone();
                                    let description = description.clone();
                                    move |_| {
                                        link.change_view(Some(ViewState::EditDescription(
                                            name.clone(),
                                            description.clone(),
                                        )))
                                    }
                                }),
                        )
                        .tip(tr!("Edit Description")),
                    )
                    .with_child(
                        Tooltip::new(
                            ActionIcon::new("fa fa-fw fa-undo")
                                .tabindex(0)
                                .aria_label(tr!("Rollback"))
                                .on_activate({
                                    let link = link.clone();
                                    let name = name.clone();
                                    move |_| {
                                        link.change_view(Some(ViewState::ConfirmRollback(
                                            name.clone(),
                                        )))
                                    }
                                }),
                        )
                        .tip(tr!("Rollback")),
                    )
                    .with_child(
                        Tooltip::new(
                            ActionIcon::new("fa fa-fw fa-trash-o")
                                .tabindex(0)
                                .aria_label(tr!("Delete"))
                                .on_activate({
                                    let link = link.clone();
                                    let name = name.clone();
                                    move |_| {
                                        link.change_view(Some(ViewState::ConfirmDelete(
                                            name.clone(),
                                        )))
                                    }
                                }),
                        )
                        .tip(tr!("Delete")),
                    )
                    .into()
            })
            .into(),
    );
    cols.push(
        DataTableColumn::new(tr!("Description"))
            .flex(2)
            .render(|e: &SnapshotTreeEntry| match e {
                SnapshotTreeEntry::Item(s) => html! { { s.description.clone() } },
                SnapshotTreeEntry::Root => html! {},
            })
            .into(),
    );

    Rc::new(cols)
}
