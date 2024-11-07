use core::convert::From;
use std::rc::Rc;

use gloo_utils::window;
use yew::{
    prelude::Html,
    virtual_dom::{Key, VComp, VNode},
};

use proxmox_yew_comp::{LoadableComponent, LoadableComponentContext, LoadableComponentMaster};
use pwt::css::{AlignItems, ColorScheme, FlexFit, JustifyContent};
use pwt::props::{ContainerBuilder, CssBorderBuilder, ExtractPrimaryKey, WidgetBuilder};
use pwt::state::{KeyedSlabTree, Selection, TreeStore};
use pwt::widget::{
    data_table::{DataTable, DataTableColumn, DataTableHeader},
    form::Field,
    ActionIcon, Column, Container, Fa, MessageBox, MessageBoxButtons, Pane, Row, SplitPane,
    Toolbar, Trigger,
};
use pwt::{prelude::*, widget::Button};

use pdm_api_types::resource::{PveLxcResource, PveNodeResource, PveQemuResource, PveResource};

pub mod lxc;
pub mod node;
pub mod qemu;
pub mod remote;
pub mod utils;
use utils::{get_remote, render_guest_tags, render_lxc_name, render_qemu_name};

#[derive(Clone, PartialEq)]
enum PveTreeNode {
    Root(bool), // loaded
    Node(PveNodeResource),
    Lxc(PveLxcResource),
    Qemu(PveQemuResource),
}

impl ExtractPrimaryKey for PveTreeNode {
    fn extract_key(&self) -> Key {
        Key::from(match self {
            PveTreeNode::Root(_) => "__root__",
            PveTreeNode::Node(node) => node.id.as_str(),
            PveTreeNode::Lxc(lxc) => lxc.id.as_str(),
            PveTreeNode::Qemu(qemu) => qemu.id.as_str(),
        })
    }
}

#[derive(Debug, Eq, PartialEq, Properties)]
pub struct PveRemote {
    remote: String,
}

impl PveRemote {
    pub fn new(remote: String) -> Self {
        yew::props!(Self { remote })
    }
}

impl Into<VNode> for PveRemote {
    fn into(self) -> VNode {
        VComp::new::<LoadableComponentMaster<PveRemoteComp>>(Rc::new(self), None).into()
    }
}

#[derive(PartialEq, Clone)]
pub enum Action {
    Start,
    Shutdown,
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Action::Start => tr!("Start"),
            Action::Shutdown => tr!("Shutdown"),
        };
        f.write_str(&text)
    }
}

#[derive(PartialEq)]
pub enum ViewState {
    Confirm(Action, String), // ID
}

pub enum Msg {
    Filter(String),
    GuestAction(Action, String), //ID
    ResourcesList(Vec<PveResource>),
    SelectedView(Option<Key>),
    SelectionChange,
}

pub struct PveRemoteComp {
    nodes: Vec<String>,
    store: TreeStore<PveTreeNode>,
    view_selection: Selection,
    loaded: bool,
    view: PveTreeNode,
    is_filtered: bool,
}

impl LoadableComponent for PveRemoteComp {
    type Message = Msg;
    type Properties = PveRemote;
    type ViewState = ViewState;

    fn create(ctx: &LoadableComponentContext<PveRemoteComp>) -> Self {
        let mut tree = KeyedSlabTree::new();
        tree.set_root(PveTreeNode::Root(false));
        let store = TreeStore::new();
        store.write().update_root_tree(tree);

        let view_selection = Selection::new().on_select(
            ctx.link()
                .callback(|selection: Selection| Msg::SelectedView(selection.selected_key())),
        );
        view_selection.select(Key::from("__root__"));

        ctx.link()
            .task_base_url(format!("/pve/remotes/{}/tasks", ctx.props().remote));

        Self {
            nodes: Vec::new(),
            loaded: false,
            store,
            view_selection,
            view: PveTreeNode::Root(false),
            is_filtered: false,
        }
    }

    fn update(
        &mut self,
        ctx: &LoadableComponentContext<PveRemoteComp>,
        msg: Self::Message,
    ) -> bool {
        let remote = &ctx.props().remote;
        match msg {
            Msg::ResourcesList(resources) => {
                let nodes = resources.iter().filter_map(|res| match res {
                    PveResource::Node(node) => Some(node.node.clone()),
                    _ => None,
                });

                self.nodes = nodes.collect();
                let mut tree = KeyedSlabTree::new();
                let mut root = tree.set_root(PveTreeNode::Root(true));
                for entry in resources {
                    match entry {
                        PveResource::Node(node_info) => {
                            let key = Key::from(node_info.id.as_str());

                            if let Some(mut node) = root.find_node_by_key_mut(&key) {
                                *node.record_mut() = PveTreeNode::Node(node_info);
                            } else {
                                root.append(PveTreeNode::Node(node_info));
                            }
                        }
                        PveResource::Qemu(qemu_info) => {
                            let node_id = format!("remote/{}/node/{}", remote, qemu_info.node);
                            let key = Key::from(node_id.as_str());
                            let mut node = match root.find_node_by_key_mut(&key) {
                                Some(node) => node,
                                None => root.append(create_empty_node(node_id)),
                            };

                            if !self.loaded {
                                node.set_expanded(true);
                            }
                            node.append(PveTreeNode::Qemu(qemu_info));
                        }
                        PveResource::Lxc(lxc_info) => {
                            let node_id = format!("remote/{}/node/{}", remote, lxc_info.node);
                            let key = Key::from(node_id.as_str());
                            let mut node = match root.find_node_by_key_mut(&key) {
                                Some(node) => node,
                                None => root.append(create_empty_node(node_id)),
                            };

                            if !self.loaded {
                                node.set_expanded(true);
                            }
                            node.append(PveTreeNode::Lxc(lxc_info));
                        }
                        _ => {} //PveResource::Storage(pve_storage_resource) => todo!(),
                    }
                }
                if !self.loaded {
                    root.set_expanded(true);
                }

                let cmp_guests =
                    |template_a, template_b, vmid_a: u32, vmid_b: u32| -> std::cmp::Ordering {
                        if template_a == template_b {
                            vmid_a.cmp(&vmid_b)
                        } else if template_a {
                            std::cmp::Ordering::Greater
                        } else {
                            std::cmp::Ordering::Less
                        }
                    };
                root.sort_by(true, |a, b| match (a, b) {
                    (PveTreeNode::Root(_), PveTreeNode::Root(_)) => std::cmp::Ordering::Equal,
                    (PveTreeNode::Root(_), _) => std::cmp::Ordering::Less,
                    (_, PveTreeNode::Root(_)) => std::cmp::Ordering::Greater,
                    (PveTreeNode::Node(a), PveTreeNode::Node(b)) => a.node.cmp(&b.node),
                    (PveTreeNode::Node(_), _) => std::cmp::Ordering::Less,
                    (_, PveTreeNode::Node(_)) => std::cmp::Ordering::Greater,
                    (PveTreeNode::Lxc(a), PveTreeNode::Lxc(b)) => {
                        cmp_guests(a.template, b.template, a.vmid, b.vmid)
                    }
                    (PveTreeNode::Lxc(_), PveTreeNode::Qemu(_)) => std::cmp::Ordering::Less,
                    (PveTreeNode::Qemu(_), PveTreeNode::Lxc(_)) => std::cmp::Ordering::Greater,
                    (PveTreeNode::Qemu(a), PveTreeNode::Qemu(b)) => {
                        cmp_guests(a.template, b.template, a.vmid, b.vmid)
                    }
                });
                self.store.write().update_root_tree(tree);
                self.loaded = true;
            }
            Msg::SelectionChange => {}
            Msg::GuestAction(action, id) => {
                let remote = ctx.props().remote.clone();
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

                match record {
                    PveTreeNode::Lxc(r) => ctx.link().spawn(async move {
                        let res = match action {
                            Action::Start => {
                                crate::pdm_client()
                                    .pve_lxc_start(&remote, Some(&r.node), r.vmid)
                                    .await
                            }
                            Action::Shutdown => {
                                crate::pdm_client()
                                    .pve_lxc_shutdown(&remote, Some(&r.node), r.vmid)
                                    .await
                            }
                        };

                        match res {
                            Ok(upid) => link.show_task_progres(upid.to_string()),
                            Err(err) => link.show_error(tr!("Error"), err.to_string(), true),
                        }
                    }),
                    PveTreeNode::Qemu(r) => ctx.link().spawn(async move {
                        let res = match action {
                            Action::Start => {
                                crate::pdm_client()
                                    .pve_qemu_start(&remote, Some(&r.node), r.vmid)
                                    .await
                            }
                            Action::Shutdown => {
                                crate::pdm_client()
                                    .pve_qemu_shutdown(&remote, Some(&r.node), r.vmid)
                                    .await
                            }
                        };

                        match res {
                            Ok(upid) => link.show_task_progres(upid.to_string()),
                            Err(err) => link.show_error(tr!("Error"), err.to_string(), true),
                        }
                    }),
                    _ => {}
                }
            }
            Msg::SelectedView(key) => {
                let store = self.store.read();
                let root = store.root().unwrap();
                let key = key.unwrap_or_else(|| Key::from("__root__"));
                if let Some(node) = root.find_node_by_key(&key) {
                    self.view = node.record().clone();
                }
            }
            Msg::Filter(text) => {
                if text.is_empty() {
                    self.store.set_filter(None);
                    self.is_filtered = false;
                } else {
                    let text = text.to_lowercase();
                    self.store.set_filter(move |node: &PveTreeNode| match node {
                        PveTreeNode::Lxc(r) => {
                            r.vmid.to_string().to_lowercase().contains(&text)
                                || r.name.to_lowercase().contains(&text)
                                || "lxc".contains(&text)
                                || r.tags.iter().any(|tag| tag.contains(&text))
                        }
                        PveTreeNode::Qemu(r) => {
                            r.vmid.to_string().to_lowercase().contains(&text)
                                || r.name.to_lowercase().contains(&text)
                                || "qemu".contains(&text)
                                || r.tags.iter().any(|tag| tag.contains(&text))
                        }
                        _ => true,
                    });
                    self.is_filtered = true;
                }
            }
        }
        true
    }

    fn main_view(&self, ctx: &LoadableComponentContext<PveRemoteComp>) -> Html {
        let props = ctx.props();

        let remote = &props.remote;

        let content: Html = match &self.view {
            PveTreeNode::Root(_) => remote::RemotePanel::new(remote.clone()).into(),
            PveTreeNode::Node(node) => {
                node::NodePanel::new(remote.clone(), node.node.clone()).into()
            }
            PveTreeNode::Qemu(qemu) => {
                qemu::QemuPanel::new(remote.clone(), qemu.node.clone(), qemu.clone()).into()
            }
            PveTreeNode::Lxc(lxc) => {
                lxc::LxcPanel::new(remote.clone(), lxc.node.clone(), lxc.clone()).into()
            }
        };

        let nav = DataTable::new(columns(ctx, self.store.clone()), self.store.clone())
            .selection(self.view_selection.clone())
            .striped(false)
            .borderless(true)
            .hover(true)
            .class(FlexFit)
            .show_header(true);

        let link = ctx.link().clone();

        SplitPane::new()
            .class(FlexFit)
            .with_child(
                Pane::new(
                    Column::new()
                        .class(FlexFit)
                        .with_child(
                            Toolbar::new()
                                .border_bottom(true)
                                .with_child(
                                    Field::new()
                                        .with_trigger(
                                            // FIXME: add `with_optional_trigger` ?
                                            Trigger::new(if self.is_filtered {
                                                "fa fa-times"
                                            } else {
                                                ""
                                            })
                                            .onclick(
                                                ctx.link().callback(|_| Msg::Filter(String::new())),
                                            ),
                                            true,
                                        )
                                        .placeholder(tr!("Filter"))
                                        .on_input(ctx.link().callback(Msg::Filter)),
                                )
                                .with_flex_spacer()
                                .with_child(
                                    Button::refresh(ctx.loading())
                                        .onclick(move |_| link.send_reload()),
                                ),
                        )
                        .with_child(nav),
                )
                .min_size(400)
                .flex(1),
            )
            .with_child(Pane::new(content).flex(1))
            .into()
    }

    fn dialog_view(
        &self,
        ctx: &LoadableComponentContext<Self>,
        view_state: &Self::ViewState,
    ) -> Option<Html> {
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
        }
    }

    fn load(
        &self,
        ctx: &LoadableComponentContext<Self>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), anyhow::Error>>>> {
        let link = ctx.link();
        let remote = ctx.props().remote.clone();
        Box::pin(async move {
            let nodes = crate::pdm_client()
                .pve_cluster_resources(&remote, None)
                .await?;
            link.send_message(Msg::ResourcesList(nodes));
            Ok(())
        })
    }
}

fn create_empty_node(node_id: String) -> PveTreeNode {
    PveTreeNode::Node(PveNodeResource {
        cgroup_mode: Default::default(),
        cpu: Default::default(),
        maxcpu: Default::default(),
        id: node_id,
        maxmem: Default::default(),
        mem: Default::default(),
        node: Default::default(),
        uptime: Default::default(),
        status: Default::default(),
    })
}

fn columns(
    ctx: &LoadableComponentContext<PveRemoteComp>,
    store: TreeStore<PveTreeNode>,
) -> Rc<Vec<DataTableHeader<PveTreeNode>>> {
    let link = ctx.link().clone();
    let remote = get_remote(link.yew_link(), &ctx.props().remote);
    let base_url = remote.and_then(|remote| {
        remote.nodes.first().and_then(|node| {
            let url = web_sys::Url::new(&format!("https://{}/", node.hostname));
            if let Ok(url) = url {
                if url.port() == "" {
                    url.set_port("8006");
                }
                Some(url)
            } else {
                None
            }
        })
    });

    let last_err = ctx.last_load_errors().is_some();

    Rc::new(vec![
        DataTableColumn::new("Type/ID")
            .flex(1)
            .tree_column(store)
            .render(move |entry: &PveTreeNode| {
                let el = match entry {
                    PveTreeNode::Root(false) if !last_err => Row::new()
                        .class(AlignItems::Center)
                        .gap(4)
                        .with_child(Container::from_tag("i").class("pwt-loading-icon"))
                        .with_child(tr!("Querying Remote...")),
                    PveTreeNode::Root(_) => Row::new()
                        .class(AlignItems::Baseline)
                        .gap(2)
                        .with_child(Fa::new("server"))
                        .with_child(tr!("Remote")),
                    PveTreeNode::Node(r) => Row::new()
                        .class(AlignItems::Baseline)
                        .gap(4)
                        .with_child(utils::render_node_status_icon(r))
                        .with_child(&r.node),
                    PveTreeNode::Qemu(r) => Row::new()
                        .class(AlignItems::Baseline)
                        .gap(2)
                        .with_child(utils::render_qemu_status_icon(r))
                        .with_child(render_qemu_name(r, true))
                        .with_child(render_guest_tags(&r.tags[..])),
                    PveTreeNode::Lxc(r) => Row::new()
                        .class(AlignItems::Baseline)
                        .gap(2)
                        .with_child(utils::render_lxc_status_icon(r))
                        .with_child(render_lxc_name(r, true))
                        .with_child(render_guest_tags(&r.tags[..])),
                };

                Container::new().with_child(el).into()
            })
            .into(),
        DataTableColumn::new(tr!("Actions"))
            .width("100px")
            .render(move |entry: &PveTreeNode| {
                let (id, local_id, status) = match entry {
                    PveTreeNode::Lxc(r) => (
                        r.id.as_str(),
                        format!("lxc/{}", r.vmid),
                        Some(r.status.as_str()),
                    ),
                    PveTreeNode::Qemu(r) => (
                        r.id.as_str(),
                        format!("qemu/{}", r.vmid),
                        Some(r.status.as_str()),
                    ),
                    PveTreeNode::Root(_) => ("root", "root".to_string(), None),
                    PveTreeNode::Node(r) => (r.id.as_str(), format!("node/{}", r.node), None),
                };

                Row::new()
                    .class(JustifyContent::FlexEnd)
                    .with_optional_child(status.map(|status| {
                        ActionIcon::new("fa fa-fw fa-power-off")
                            .disabled(status != "running")
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
                            .class(ColorScheme::Error)
                    }))
                    .with_optional_child(status.map(|status| {
                        ActionIcon::new("fa fa-fw fa-play")
                            .disabled(status != "stopped")
                            .on_activate({
                                let id = id.to_string();
                                let link = link.clone();
                                move |_| {
                                    link.change_view(Some(ViewState::Confirm(
                                        Action::Start,
                                        id.to_string(),
                                    )));
                                }
                            })
                            .class(ColorScheme::Success)
                    }))
                    .with_optional_child(base_url.clone().map(|url| {
                        ActionIcon::new("fa fa-chevron-right").on_activate(move |()| {
                            url.set_hash(&format!("v1::={local_id}"));
                            let _ = window().open_with_url(&url.href());
                        })
                    }))
                    .into()
            })
            .into(),
    ])
}
