use core::convert::From;
use std::rc::Rc;

use gloo_utils::window;
use yew::{
    prelude::Html,
    virtual_dom::{Key, VComp, VNode},
};

use proxmox_yew_comp::{
    LoadableComponent, LoadableComponentContext, LoadableComponentMaster, LoadableComponentScope,
    LoadableComponentScopeExt, LoadableComponentState,
};
use pwt::css::{AlignItems, ColorScheme, FlexFit, FontStyle, JustifyContent};
use pwt::props::{ContainerBuilder, CssBorderBuilder, ExtractPrimaryKey, WidgetBuilder};
use pwt::state::{KeyedSlabTree, NavigationContext, NavigationContextExt, Selection, TreeStore};
use pwt::widget::{
    ActionIcon, Column, Container, Fa, MessageBox, MessageBoxButtons, Row, Toolbar, Tooltip,
    Trigger,
    data_table::{DataTable, DataTableColumn, DataTableHeader},
    form::Field,
};
use pwt::{prelude::*, widget::Button};

use pdm_api_types::{
    RemoteUpid,
    resource::{AsResourceView, PveNodeResource, PveResource},
};

use crate::{
    get_deep_url,
    renderer::{
        render_resource_extra_info, render_resource_name, render_status_icon, render_tree_column,
    },
    widget::MigrateWindow,
};

use super::{GuestInfo, GuestType, utils};

#[derive(Clone, PartialEq)]
pub enum PveTreeNode {
    Root,
    Resource(PveResource),
}

impl ExtractPrimaryKey for PveTreeNode {
    fn extract_key(&self) -> Key {
        Key::from(match self {
            PveTreeNode::Root => "__root__",
            PveTreeNode::Resource(resource) => resource.as_resource_view().global_id(),
        })
    }
}

impl PveTreeNode {
    fn get_path(&self) -> String {
        match self {
            PveTreeNode::Root => "datacenter".to_string(),
            PveTreeNode::Resource(resource) => match resource {
                PveResource::Node(node) => format!("node+{}", node.node),
                PveResource::Lxc(lxc) => format!("guest+{}", lxc.vmid),
                PveResource::Qemu(qemu) => format!("guest+{}", qemu.vmid),
                PveResource::Storage(storage) => {
                    format!("storage+{}+{}", storage.node, storage.storage)
                }
                PveResource::Network(network) => format!("network+{}", network.name()),
            },
        }
    }
}

#[derive(PartialEq, Properties)]
pub struct PveTree {
    remote: String,

    resources: Rc<Vec<PveResource>>,

    loading: bool,

    on_select: Callback<PveTreeNode>,

    on_reload_click: Callback<()>,
}

impl PveTree {
    pub fn new(
        remote: String,
        resources: Rc<Vec<PveResource>>,
        loading: bool,
        on_select: impl Into<Callback<PveTreeNode>>,
        on_reload_click: impl Into<Callback<()>>,
    ) -> Self {
        yew::props!(Self {
            remote,
            resources,
            loading,
            on_select: on_select.into(),
            on_reload_click: on_reload_click.into(),
        })
    }
}

impl From<PveTree> for VNode {
    fn from(val: PveTree) -> Self {
        VComp::new::<LoadableComponentMaster<PveTreeComp>>(Rc::new(val), None).into()
    }
}

#[derive(PartialEq, Clone)]
pub enum Action {
    Start,
    Shutdown,
    Resume,
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Action::Start => tr!("Start"),
            Action::Shutdown => tr!("Shutdown"),
            Action::Resume => tr!("Resume"),
        };
        f.write_str(&text)
    }
}

#[derive(PartialEq)]
pub enum ViewState {
    Confirm(Action, String), // ID
    /// Open the migration dialog for the given guest, carrying its current node so the
    /// target-node selector can grey out (and reject) that entry.
    MigrateWindow(GuestInfo, String),
}

pub enum Msg {
    Filter(String),
    GuestAction(Action, String), //ID
    KeySelected(Option<Key>),
    RouteChanged(String),
}

pub struct PveTreeComp {
    state: LoadableComponentState<ViewState>,
    columns: Rc<Vec<DataTableHeader<PveTreeNode>>>,
    store: TreeStore<PveTreeNode>,
    loaded: bool,
    filter: String,
    _nav_handle: ContextHandle<NavigationContext>,
    view_selection: Selection,
}

pwt::impl_deref_mut_property!(PveTreeComp, state, LoadableComponentState<ViewState>);

impl PveTreeComp {
    fn load_tree(&mut self, ctx: &LoadableComponentContext<PveTreeComp>) {
        let remote = ctx.props().remote.clone();
        let resources = ctx.props().resources.as_ref();
        let mut tree = KeyedSlabTree::new();
        let mut root = tree.set_root(PveTreeNode::Root);
        for entry in resources {
            match entry {
                PveResource::Node(node_info) => {
                    let key = Key::from(node_info.id.as_str());

                    if let Some(mut node) = root.find_node_by_key_mut(&key) {
                        *node.record_mut() = PveTreeNode::Resource(entry.clone());
                    } else {
                        root.append(PveTreeNode::Resource(entry.clone()));
                    }
                }
                PveResource::Qemu(_) | PveResource::Lxc(_) | PveResource::Storage(_) => {
                    // PVE resources always have a node
                    let node = entry.as_resource_view().node().unwrap();
                    let node_id = format!("remote/{remote}/node/{node}");

                    let key = Key::from(node_id.as_str());
                    let mut node = match root.find_node_by_key_mut(&key) {
                        Some(node) => node,
                        None => root.append(create_empty_node(node_id)),
                    };

                    if !self.loaded {
                        node.set_expanded(true);
                    }
                    node.append(PveTreeNode::Resource(entry.clone()));
                }
                PveResource::Network(_) => {}
            }
        }
        if !self.loaded {
            root.set_expanded(true);
        }

        let cmp_guests = |template_a, template_b, vmid_a: u32, vmid_b: u32| -> std::cmp::Ordering {
            if template_a == template_b {
                vmid_a.cmp(&vmid_b)
            } else if template_a {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Less
            }
        };
        root.sort_by(true, |a, b| match (a, b) {
            (PveTreeNode::Root, PveTreeNode::Root) => std::cmp::Ordering::Equal,
            (PveTreeNode::Root, _) => std::cmp::Ordering::Less,
            (_, PveTreeNode::Root) => std::cmp::Ordering::Greater,
            (PveTreeNode::Resource(a), PveTreeNode::Resource(b)) => match (a, b) {
                (PveResource::Node(a), PveResource::Node(b)) => a.node.cmp(&b.node),
                (PveResource::Node(_), _) => std::cmp::Ordering::Less,
                (_, PveResource::Node(_)) => std::cmp::Ordering::Greater,
                (PveResource::Lxc(a), PveResource::Lxc(b)) => {
                    cmp_guests(a.template, b.template, a.vmid, b.vmid)
                }
                (PveResource::Lxc(_), PveResource::Qemu(_)) => std::cmp::Ordering::Less,
                (PveResource::Qemu(_), PveResource::Lxc(_)) => std::cmp::Ordering::Greater,
                (PveResource::Qemu(a), PveResource::Qemu(b)) => {
                    cmp_guests(a.template, b.template, a.vmid, b.vmid)
                }
                (PveResource::Lxc(_) | PveResource::Qemu(_), PveResource::Storage(_)) => {
                    std::cmp::Ordering::Less
                }
                (PveResource::Storage(_), PveResource::Lxc(_) | PveResource::Qemu(_)) => {
                    std::cmp::Ordering::Greater
                }
                (PveResource::Storage(a), PveResource::Storage(b)) => a.id.cmp(&b.id),
                (PveResource::Network(a), PveResource::Network(b)) => a.id().cmp(b.id()),
                (PveResource::Network(_), _) => std::cmp::Ordering::Greater,
                (_, PveResource::Network(_)) => std::cmp::Ordering::Less,
            },
        });
        let first_id = root
            .children()
            .next()
            .map(|c| c.key())
            .unwrap_or(Key::from("__root__"));
        let select_key = self
            .view_selection
            .selected_key()
            .unwrap_or(first_id.clone());
        if !self.loaded {
            if let Some(node) = tree.lookup_node(&select_key) {
                self.view_selection.select(select_key);
                ctx.props().on_select.emit(node.record().clone());
            } else {
                self.view_selection.select(first_id);
            }
        }
        self.store.write().update_root_tree(tree);
        self.store.write().set_view_root(true);
        self.loaded = true;
    }
}

fn get_base_url(remote: &str) -> AttrValue {
    format!("/pve/remotes/{remote}/tasks").into()
}

impl LoadableComponent for PveTreeComp {
    type Message = Msg;
    type Properties = PveTree;
    type ViewState = ViewState;

    fn create(ctx: &LoadableComponentContext<PveTreeComp>) -> Self {
        let mut tree = KeyedSlabTree::new();
        tree.set_root(PveTreeNode::Root);
        let store = TreeStore::new();
        store.write().update_root_tree(tree);

        let link = ctx.link();

        let view_selection = Selection::new().on_select(
            link.callback(|selection: Selection| Msg::KeySelected(selection.selected_key())),
        );

        link.repeated_load(3000);

        let (_nav_ctx, _nav_handle) = ctx
            .link()
            .context::<NavigationContext>(Callback::from({
                let link = ctx.link().clone();
                move |nav_ctx: NavigationContext| {
                    let path = nav_ctx.path();
                    link.send_message(Msg::RouteChanged(path));
                }
            }))
            .unwrap();

        let path = _nav_ctx.path();
        ctx.link().send_message(Msg::RouteChanged(path));

        let mut state = LoadableComponentState::new();
        state.set_task_base_url(get_base_url(&ctx.props().remote));

        Self {
            state,
            columns: columns(
                link.clone(),
                store.clone(),
                ctx.props().remote.clone(),
                ctx.props().loading,
            ),
            loaded: false,
            store,
            filter: String::new(),
            _nav_handle,
            view_selection,
        }
    }

    fn update(&mut self, ctx: &LoadableComponentContext<PveTreeComp>, msg: Self::Message) -> bool {
        let remote = &ctx.props().remote;
        match msg {
            Msg::GuestAction(action, id) => {
                let remote = remote.clone();
                let store = self.store.read();
                let root = store.root();
                if root.is_none() {
                    return false;
                }
                let root = root.unwrap();
                let node = root.find_node_by_key(&Key::from(id.as_str()));
                if node.is_none() {
                    return false;
                }
                let node = node.unwrap();
                let record = node.record().clone();
                let link = ctx.link().clone();

                let PveTreeNode::Resource(resource) = record else {
                    return false;
                };
                let resource = resource.as_resource_view();

                let Some(guest_node) = resource.node().map(|n| n.to_string()) else {
                    return false;
                };
                let Some(vmid) = resource.vmid() else {
                    return false;
                };
                let Some(guest_type) = resource.guest_type() else {
                    return false;
                };

                ctx.link().spawn(async move {
                    let client = crate::pdm_client();
                    let node = Some(guest_node.as_str());
                    let res = match (guest_type.into(), action) {
                        (GuestType::Qemu, Action::Start) => {
                            client.pve_qemu_start(&remote, node, vmid).await
                        }
                        (GuestType::Qemu, Action::Shutdown) => {
                            client.pve_qemu_shutdown(&remote, node, vmid).await
                        }
                        (GuestType::Qemu, Action::Resume) => {
                            client.pve_qemu_resume(&remote, node, vmid).await
                        }
                        (GuestType::Lxc, Action::Start) => {
                            client.pve_lxc_start(&remote, node, vmid).await
                        }
                        (GuestType::Lxc, Action::Shutdown) => {
                            client.pve_lxc_shutdown(&remote, node, vmid).await
                        }
                        // LXC resume is not exposed yet, the UI never offers it.
                        (GuestType::Lxc, Action::Resume) => return,
                    };

                    match res {
                        Ok(upid) => link.show_task_progress(upid.to_string()),
                        Err(err) => link.show_error(tr!("Error"), err.to_string(), true),
                    }
                });
            }
            Msg::KeySelected(key) => {
                let key = key.unwrap_or_else(|| Key::from("__root__"));
                let store = self.store.read();
                let root = store.root().unwrap();

                if let Some(node) = root.find_node_by_key(&key) {
                    let record = node.record().clone();
                    if let Some(nav) = ctx.link().nav_context() {
                        let new_path = record.get_path();
                        let current_path = nav.path();
                        if current_path != new_path {
                            ctx.link().push_relative_route(&new_path);
                        }
                    }

                    ctx.props().on_select.emit(record);
                }
            }
            Msg::RouteChanged(path) => {
                let key = if path == "_" || path == "datacenter" || path == "" {
                    Key::from("__root__")
                } else {
                    Key::from(format!(
                        "remote/{}/{}",
                        ctx.props().remote,
                        path.replace("+", "/")
                    ))
                };
                self.view_selection.select(key);
            }
            Msg::Filter(text) => {
                self.filter = text;
                if self.filter.is_empty() {
                    self.store.set_filter(None);
                } else {
                    let text = self.filter.to_lowercase();
                    self.store.set_filter(move |node: &PveTreeNode| match node {
                        PveTreeNode::Resource(PveResource::Lxc(r)) => {
                            r.vmid.to_string().to_lowercase().contains(&text)
                                || r.name.to_lowercase().contains(&text)
                                || "lxc".contains(&text)
                                || r.tags.iter().any(|tag| tag.contains(&text))
                        }
                        PveTreeNode::Resource(PveResource::Qemu(r)) => {
                            r.vmid.to_string().to_lowercase().contains(&text)
                                || r.name.to_lowercase().contains(&text)
                                || "qemu".contains(&text)
                                || r.tags.iter().any(|tag| tag.contains(&text))
                        }
                        PveTreeNode::Resource(PveResource::Storage(r)) => {
                            r.storage.to_string().to_lowercase().contains(&text)
                                || "storage".contains(&text)
                        }
                        // always show tree root node to ensure tree does not look odd.
                        // For now also always show all nodes (should we filter those without any
                        // matches for the node or for it's sub elements?
                        PveTreeNode::Root | PveTreeNode::Resource(PveResource::Node(_)) => true,
                        // don't show the networks
                        PveTreeNode::Resource(PveResource::Network(_)) => false,
                    });
                }
            }
        }
        true
    }

    fn changed(
        &mut self,
        ctx: &LoadableComponentContext<Self>,
        _old_props: &Self::Properties,
    ) -> bool {
        let props = ctx.props();

        self.state.set_task_base_url(get_base_url(&props.remote));

        if props.resources != _old_props.resources {
            self.load_tree(ctx);
        }

        self.columns = columns(
            ctx.link().clone(),
            self.store.clone(),
            props.remote.clone(),
            props.loading,
        );

        true
    }

    fn main_view(&self, ctx: &LoadableComponentContext<PveTreeComp>) -> Html {
        let nav = DataTable::new(Rc::clone(&self.columns), self.store.clone())
            .selection(self.view_selection.clone())
            .striped(false)
            .borderless(true)
            .hover(true)
            .class(FlexFit)
            .show_header(false);

        let link = ctx.link();

        Column::new()
            .class(FlexFit)
            .with_child(
                Toolbar::new()
                    .border_bottom(true)
                    .with_child(
                        Row::new()
                            .class(AlignItems::Baseline)
                            .class(FontStyle::TitleMedium)
                            .gap(2)
                            .with_child(Fa::new("server"))
                            .with_child(tr!("Resources")),
                    )
                    .with_child(
                        Field::new()
                            .value(self.filter.clone())
                            .with_trigger(
                                // FIXME: add `with_optional_trigger` ?
                                Trigger::new(if !self.filter.is_empty() {
                                    "fa fa-times"
                                } else {
                                    ""
                                })
                                .on_activate(link.callback(|_| Msg::Filter(String::new()))),
                                true,
                            )
                            .placeholder(tr!("Filter"))
                            .on_input(link.callback(Msg::Filter)),
                    )
                    .with_flex_spacer()
                    .with_child(Button::refresh(ctx.props().loading).on_activate({
                        let on_reload_click = ctx.props().on_reload_click.clone();
                        move |_| {
                            on_reload_click.emit(());
                        }
                    })),
            )
            .with_child(nav)
            .into()
    }

    fn dialog_view(
        &self,
        ctx: &LoadableComponentContext<Self>,
        view_state: &Self::ViewState,
    ) -> Option<Html> {
        let props = ctx.props();
        match view_state {
            ViewState::Confirm(action, id) => {
                let action = action.clone();
                Some(
                    MessageBox::new(tr!("Confirm"), format!("{} - {}", action, id))
                        .buttons(MessageBoxButtons::YesNo)
                        .on_close({
                            let id = id.clone();
                            let link = ctx.link().clone();
                            move |confirm| {
                                if confirm {
                                    link.send_message(Msg::GuestAction(
                                        action.clone(),
                                        id.to_string(),
                                    ));
                                }
                                link.change_view(None);
                            }
                        })
                        .into(),
                )
            }
            ViewState::MigrateWindow(guest_info, source_node) => Some(
                MigrateWindow::new(props.remote.clone(), *guest_info)
                    .source_node(AttrValue::from(source_node.clone()))
                    .on_close(ctx.link().change_view_callback(|_| None))
                    .on_submit({
                        let link = ctx.link().clone();
                        move |upid: RemoteUpid| link.show_task_progress(upid.to_string())
                    })
                    .into(),
            ),
        }
    }

    fn load(
        &self,
        _ctx: &LoadableComponentContext<Self>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), anyhow::Error>>>> {
        Box::pin(async move { Ok(()) })
    }
}

fn create_empty_node(node_id: String) -> PveTreeNode {
    PveTreeNode::Resource(PveResource::Node(PveNodeResource {
        cgroup_mode: Default::default(),
        cpu: Default::default(),
        maxcpu: Default::default(),
        id: node_id,
        maxmem: Default::default(),
        mem: Default::default(),
        node: Default::default(),
        uptime: Default::default(),
        status: Default::default(),
        level: Default::default(),
        ha_state: Default::default(),
    }))
}

fn columns(
    link: LoadableComponentScope<PveTreeComp>,
    store: TreeStore<PveTreeNode>,
    remote: String,
    loading: bool,
) -> Rc<Vec<DataTableHeader<PveTreeNode>>> {
    let loading = match store.read().root() {
        Some(root) => loading && root.children_count() == 0,
        None => loading,
    };
    Rc::new(vec![
        DataTableColumn::new("Type/ID")
            .flex(1)
            .tree_column(store)
            .render(move |entry: &PveTreeNode| {
                let mut extra = None;
                let (icon, text) = match entry {
                    PveTreeNode::Root if loading => (
                        Container::from_tag("i").class("pwt-loading-icon"),
                        tr!("Querying Remote..."),
                    ),
                    PveTreeNode::Root => (
                        Container::new().with_child(Fa::new("server")),
                        tr!("Datacenter"),
                    ),
                    PveTreeNode::Resource(r) => {
                        extra = render_resource_extra_info(r);
                        (render_status_icon(r), render_resource_name(r, true))
                    }
                };

                render_tree_column(icon.into(), text)
                    .with_optional_child(extra)
                    .into()
            })
            .into(),
        DataTableColumn::new(tr!("Actions"))
            .width("180px")
            .render(move |entry: &PveTreeNode| {
                let (id, local_id, guest_info, node) = match entry {
                    PveTreeNode::Root => ("root", "root".to_string(), None, None),
                    PveTreeNode::Resource(r) => match r {
                        PveResource::Lxc(r) => {
                            let guest_info = GuestInfo::new(GuestType::Lxc, r.vmid);
                            let local_id = guest_info.local_id();
                            (
                                r.id.as_str(),
                                local_id,
                                Some((guest_info, r.status.as_str(), r.template)),
                                Some(r.node.clone()),
                            )
                        }
                        PveResource::Qemu(r) => {
                            let guest_info = GuestInfo::new(GuestType::Qemu, r.vmid);
                            let local_id = guest_info.local_id();
                            (
                                r.id.as_str(),
                                local_id,
                                Some((guest_info, r.status.as_str(), r.template)),
                                Some(r.node.clone()),
                            )
                        }
                        PveResource::Node(_)
                        | PveResource::Storage(_)
                        | PveResource::Network(_) => (
                            r.as_resource_view().global_id(),
                            r.as_resource_view().id(),
                            None,
                            r.as_resource_view().node().map(|n| n.to_string()),
                        ),
                    },
                };

                Row::new()
                    .class(JustifyContent::FlexEnd)
                    .with_optional_child(guest_info.and_then(|(_, status, template)| {
                        if template {
                            return None;
                        }
                        // a paused guest is still live and can be shut down
                        let disabled = !utils::guest_is_live(status);
                        let icon = Tooltip::new(
                            ActionIcon::new("fa fa-fw fa-power-off")
                                .disabled(disabled)
                                .on_activate({
                                    let id = id.to_string();
                                    let link = link.clone();
                                    move |_| {
                                        link.change_view(Some(ViewState::Confirm(
                                            Action::Shutdown,
                                            id.to_string(),
                                        )))
                                    }
                                })
                                .class((!disabled).then_some(ColorScheme::Error)),
                        )
                        .tip(tr!("Shutdown"));
                        Some(icon)
                    }))
                    .with_optional_child(guest_info.and_then(|(info, status, template)| {
                        if template {
                            return None;
                        }
                        // resume is QEMU-only; LXC keeps its disabled Start button
                        let resume = info.guest_type == GuestType::Qemu
                            && matches!(status, "paused" | "prelaunch" | "suspended");
                        let (action, scheme, label) = if resume {
                            (Action::Resume, ColorScheme::Warning, tr!("Resume"))
                        } else {
                            (Action::Start, ColorScheme::Success, tr!("Start"))
                        };
                        let disabled = !resume && utils::guest_is_live(status);
                        let icon = Tooltip::new(
                            ActionIcon::new("fa fa-fw fa-play")
                                .disabled(disabled)
                                .on_activate({
                                    let id = id.to_string();
                                    let link = link.clone();
                                    move |_| {
                                        link.change_view(Some(ViewState::Confirm(
                                            action.clone(),
                                            id.to_string(),
                                        )));
                                    }
                                })
                                .class((!disabled).then_some(scheme)),
                        )
                        .tip(label);
                        Some(icon)
                    }))
                    .with_optional_child(guest_info.and_then(|(guest_info, _, _)| {
                        let source_node = node.clone()?;
                        Some(
                            Tooltip::new(
                                ActionIcon::new("fa fa-fw fa-paper-plane-o")
                                    .aria_label(tr!("Migrate"))
                                    .on_activate({
                                        let link = link.clone();
                                        move |_| {
                                            link.change_view(Some(ViewState::MigrateWindow(
                                                guest_info,
                                                source_node.clone(),
                                            )))
                                        }
                                    }),
                            )
                            .tip(tr!("Migrate")),
                        )
                    }))
                    .with_child(
                        Tooltip::new(
                            ActionIcon::new("fa fa-external-link")
                                .aria_label(tr!("Open in PVE UI"))
                                .on_activate({
                                    let link = link.clone();
                                    let remote = remote.clone();
                                    move |_| {
                                        // there must be a remote with a connections config if were already here
                                        if let Some(url) =
                                            get_deep_url(&link, &remote, node.as_deref(), &local_id)
                                        {
                                            let _ = window().open_with_url(&url.href());
                                        }
                                    }
                                }),
                        )
                        .tip(tr!("Open in PVE UI")),
                    )
                    .into()
            })
            .into(),
    ])
}
