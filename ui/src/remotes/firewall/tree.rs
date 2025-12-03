use futures::Future;
use gloo_utils::window;
use std::pin::Pin;
use std::rc::Rc;
use yew::{ContextHandle, Html};

use proxmox_yew_comp::{LoadableComponent, LoadableComponentContext};
use pwt::css;
use pwt::prelude::*;
use pwt::props::{FieldBuilder, WidgetBuilder};
use pwt::state::{Selection, TreeStore};
use pwt::tr;
use pwt::widget::data_table::DataTable;
use pwt::widget::form::{Combobox, Field};
use pwt::widget::{Button, Container, Panel, TabBarItem, TabPanel, Toolbar, Trigger};

use crate::RemoteList;

use super::columns::create_columns;
use super::types::{
    FirewallError, GuestEntry, LoadState, NodeEntry, RemoteEntry, Scope, TreeEntry,
};
use super::ui_helpers::PanelConfig;

use pdm_api_types::firewall::{GuestKind, NodeFirewallStatus, RemoteFirewallStatus};
use pdm_api_types::remotes::RemoteType;
use std::cmp::Ordering;

use proxmox_yew_comp::configuration::pve::{
    FirewallOptionsClusterPanel, FirewallOptionsGuestPanel, FirewallOptionsNodePanel,
};
use proxmox_yew_comp::form::pve::PveGuestType;

fn create_loading_tree() -> pwt::state::SlabTree<TreeEntry> {
    let mut tree = pwt::state::SlabTree::new();
    tree.set_root(TreeEntry::Root);
    tree
}

fn build_tree_from_remotes(
    mut remote_statuses: Vec<RemoteFirewallStatus>,
) -> pwt::state::SlabTree<TreeEntry> {
    let mut tree = pwt::state::SlabTree::new();
    tree.set_root(TreeEntry::Root);

    if let Some(mut root) = tree.root_mut() {
        root.set_expanded(true);

        remote_statuses.sort_by(|a, b| a.remote.cmp(&b.remote));

        for remote_status in remote_statuses {
            add_remote_to_tree(&mut root, remote_status);
        }
    }

    tree
}

fn add_remote_to_tree(
    root: &mut pwt::state::SlabTreeNodeMut<TreeEntry>,
    remote_status: RemoteFirewallStatus,
) {
    let RemoteFirewallStatus {
        remote: remote_name,
        status: cluster_fw_status,
        mut nodes,
    } = remote_status;

    let cluster_is_enabled = cluster_fw_status
        .as_ref()
        .map(|s| s.enabled)
        .unwrap_or(true);

    let remote_entry = TreeEntry::Remote(RemoteEntry {
        name: remote_name.clone(),
        status: cluster_fw_status,
    });

    let mut remote_handle = root.append(remote_entry);
    remote_handle.set_expanded(cluster_is_enabled);

    nodes.sort_by(|a, b| a.node.cmp(&b.node));

    for node_status in nodes {
        let NodeFirewallStatus {
            node: node_name,
            status: node_firewall_status,
            mut guests,
        } = node_status;

        let node_entry = TreeEntry::Node(NodeEntry {
            remote: remote_name.clone(),
            name: node_name.clone(),
            status: node_firewall_status,
            masked: !cluster_is_enabled,
        });

        let mut node_handle = remote_handle.append(node_entry);

        node_handle.set_expanded(!guests.is_empty());

        guests.sort_by(|a, b| match (a.kind, b.kind) {
            (GuestKind::Lxc, GuestKind::Qemu) => Ordering::Less,
            (GuestKind::Qemu, GuestKind::Lxc) => Ordering::Greater,
            _ => a.vmid.cmp(&b.vmid),
        });

        for guest in guests {
            let kind = guest.kind;
            let guest_entry = GuestEntry::new(
                guest,
                node_name.clone(),
                remote_name.clone(),
                !cluster_is_enabled,
            );

            let tree_entry = TreeEntry::Guest(guest_entry, kind);

            node_handle.append(tree_entry);
        }
    }
}

fn sort_entries(a: &TreeEntry, b: &TreeEntry) -> Ordering {
    (a.sort_rank(), a.name()).cmp(&(b.sort_rank(), b.name()))
}

pub enum Msg {
    DataLoaded {
        generation: usize,
        data: Vec<pdm_api_types::firewall::RemoteFirewallStatus>,
    },
    RemoteListChanged,
    Reload,
    FilterChanged(String),
    ScopeChanged(Scope),
    RemotesLoaded(Vec<String>),
    NodesLoaded {
        generation: usize,
        nodes: Vec<String>,
    },
    SelectionChanged,
    TabChanged,
    ToggleTreePanel,
    Error(FirewallError),
    NoOp,
}

pub struct FirewallTreeComponent {
    store: TreeStore<TreeEntry>,
    selection: Selection,
    tab_selection: Selection,
    _context_listener: ContextHandle<RemoteList>,
    filter_text: String,
    scope: Scope,
    available_remotes: Vec<String>,
    available_nodes: Vec<String>,
    options_loading: bool,
    load_state: LoadState,
    selected_entry: Option<TreeEntry>,
    tree_collapsed: bool,
}

impl FirewallTreeComponent {
    fn reset_tree_for_loading(&mut self) {
        let tree = create_loading_tree();
        self.store.write().update_root_tree(tree);
        self.store.write().set_view_root(true);
    }

    fn clear_selection(&mut self) {
        self.selected_entry = None;
    }

    fn get_pve_url(&self, ctx: &LoadableComponentContext<Self>, tab: &str) -> Option<String> {
        let entry = self.selected_entry.as_ref()?;
        let (remote, node, vmid, kind) = match entry {
            TreeEntry::Remote(r) => (r.name.as_str(), None, None, None),
            TreeEntry::Node(n) => (n.remote.as_str(), Some(n.name.as_str()), None, None),
            TreeEntry::Guest(g, kind) => (
                g.remote.as_str(),
                Some(g.node.as_str()),
                Some(g.guest.vmid),
                Some(kind),
            ),
            _ => return None,
        };

        let is_options = tab == "options";
        let index = if is_options { 36 } else { 32 };

        match (node, vmid, kind) {
            (None, None, _) => {
                let id = format!("v1:0:18:4:::::::{index}");
                let url = crate::get_deep_url_low_level(ctx.link().yew_link(), remote, None, &id)?;
                Some(url.href())
            }
            (Some(node), None, _) => {
                let id = format!("node/{node}:4:{index}");
                let url = crate::get_deep_url(ctx.link().yew_link(), remote, Some(node), &id)?;
                Some(url.href())
            }
            (Some(node), Some(vmid), Some(kind)) => {
                let id = match kind {
                    GuestKind::Lxc => format!("lxc/{vmid}:4::::::{index}"),
                    GuestKind::Qemu => format!("qemu/{vmid}:4:::::{index}"),
                };
                let url = crate::get_deep_url(ctx.link().yew_link(), remote, Some(node), &id)?;
                Some(url.href())
            }
            _ => None,
        }
    }

    fn handle_scope_change(&mut self, ctx: &LoadableComponentContext<Self>, new_scope: Scope) {
        let remote_changed = self.scope.remote_name() != new_scope.remote_name();

        if remote_changed && new_scope.remote_name().is_some() {
            self.scope = match &new_scope {
                Scope::Node { remote, .. } | Scope::Remote { name: remote } => Scope::Remote {
                    name: remote.clone(),
                },
                Scope::All => Scope::All,
            };
            self.available_nodes.clear();
            self.start_node_load(ctx);
        } else {
            self.scope = new_scope;
        }

        self.clear_selection();
        self.reset_tree_for_loading();
        let _generation = self.load_state.start_data_load();
        ctx.link().send_reload();
    }

    fn start_node_load(&mut self, ctx: &LoadableComponentContext<Self>) {
        if let Some(remote) = self.scope.remote_name() {
            let generation = self.load_state.start_nodes_load();
            let link = ctx.link().clone();
            let remote = remote.to_string();

            ctx.link().spawn(async move {
                match load_nodes_for_remote(remote).await {
                    Ok((_remote, nodes)) => {
                        link.send_message(Msg::NodesLoaded { generation, nodes });
                    }
                    Err(err) => {
                        link.send_message(Msg::Error(err));
                    }
                }
            });
        }
    }

    fn render_tree_panel(&self, ctx: &LoadableComponentContext<Self>) -> Panel {
        let columns = create_columns(self.store.clone(), ctx.loading(), &self.scope);
        let table = DataTable::new(columns, self.store.clone())
            .selection(self.selection.clone())
            .striped(false)
            .borderless(true)
            .show_header(false)
            .class(css::FlexFit);

        let title_toolbar = Toolbar::new()
            .border_bottom(true)
            .with_child(
                pwt::widget::Row::new()
                    .class(pwt::css::AlignItems::Baseline)
                    .class(pwt::css::FontStyle::TitleMedium)
                    .gap(2)
                    .with_child(pwt::widget::Fa::new("shield"))
                    .with_child(tr!("Status")),
            )
            .with_child(
                Field::new()
                    .value(self.filter_text.clone())
                    .with_trigger(
                        Trigger::new(if !self.filter_text.is_empty() {
                            "fa fa-times"
                        } else {
                            ""
                        })
                        .on_activate(ctx.link().callback(|_| Msg::FilterChanged(String::new()))),
                        true,
                    )
                    .placeholder(tr!("Filter"))
                    .on_input(ctx.link().callback(Msg::FilterChanged)),
            )
            .with_child(
                Button::new_icon("fa fa-angle-double-left")
                    .onclick(ctx.link().callback(|_| Msg::ToggleTreePanel))
                    .aria_label(tr!("Hide tree panel"))
                    .disabled(self.selected_entry.is_none()),
            );

        let scope_toolbar = Toolbar::new()
            .with_child(create_remote_combobox(
                ctx,
                &self.available_remotes,
                self.options_loading,
                &self.scope,
            ))
            .with_child(create_node_combobox(
                ctx,
                &self.available_nodes,
                self.options_loading,
                &self.scope,
            ))
            .with_flex_spacer()
            .with_child(
                Button::refresh(ctx.loading()).onclick(ctx.link().callback(|_| Msg::Reload)),
            );

        let column = pwt::widget::Column::new()
            .class(css::FlexFit)
            .with_child(title_toolbar)
            .with_child(scope_toolbar)
            .with_child(table);

        Panel::new().border(true).with_child(column)
    }

    fn render_content_panel(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        let entry = match &self.selected_entry {
            Some(entry) => entry,
            None => return PanelConfig::for_no_selection().content.into(),
        };

        let config = PanelConfig::from_entry(entry, self.load_state.data_generation);

        let title = if self.tree_collapsed {
            let expand_button: Html = Button::new_icon("fa fa-angle-double-right")
                .onclick(ctx.link().callback(|_| Msg::ToggleTreePanel))
                .aria_label(tr!("Show tree panel"))
                .into();

            pwt::widget::Row::new()
                .gap(2)
                .class(pwt::css::AlignItems::Baseline)
                .with_child(expand_button)
                .with_child(config.title)
                .into()
        } else {
            config.title
        };

        let current_tab = self
            .tab_selection
            .selected_key()
            .map(|k| k.to_string())
            .unwrap_or_else(|| "rules".to_string());

        let pve_url = self.get_pve_url(ctx, &current_tab);

        let mut tab_panel = TabPanel::new()
            .selection(self.tab_selection.clone())
            .class(css::FlexFit)
            .class(css::ColorScheme::Neutral)
            .title(title)
            .tool(
                Button::new(tr!("Open Web UI"))
                    .icon_class("fa fa-external-link")
                    .on_activate(move |_| {
                        if let Some(url) = &pve_url {
                            let _ = window().open_with_url(url);
                        }
                    }),
            )
            .with_item_builder(
                TabBarItem::new()
                    .key("rules")
                    .label(tr!("Rules"))
                    .icon_class("fa fa-list"),
                {
                    let content = config.content;
                    let key = format!(
                        "{}-{}-{}",
                        entry.type_name(),
                        entry.name(),
                        self.load_state.data_generation
                    );
                    move |_| {
                        Container::new()
                            .key(key.clone())
                            .class(css::FlexFit)
                            .with_child(content.clone())
                            .into()
                    }
                },
            );

        let add_options_tab = |panel: TabPanel, content: Html| {
            panel.with_item_builder(
                TabBarItem::new()
                    .key("options")
                    .label(tr!("Options"))
                    .icon_class("fa fa-cog"),
                move |_| content.clone(),
            )
        };

        tab_panel = match entry {
            TreeEntry::Remote(remote) => {
                let remote_name = remote.name.clone();
                add_options_tab(
                    tab_panel,
                    FirewallOptionsClusterPanel::new()
                        .remote(remote_name.clone())
                        .readonly(true)
                        .into(),
                )
            }
            TreeEntry::Node(node) => {
                let remote_name = node.remote.clone();
                let node_name = node.name.clone();
                add_options_tab(
                    tab_panel,
                    FirewallOptionsNodePanel::new(yew::AttrValue::from(node_name.clone()))
                        .remote(remote_name.clone())
                        .readonly(true)
                        .into(),
                )
            }
            TreeEntry::Guest(guest, kind) => {
                let remote_name = guest.remote.clone();
                let node_name = guest.node.clone();
                let vmid = guest.guest.vmid;
                let guest_type = match kind {
                    GuestKind::Lxc => PveGuestType::Lxc,
                    GuestKind::Qemu => PveGuestType::Qemu,
                };

                add_options_tab(
                    tab_panel,
                    FirewallOptionsGuestPanel::new(
                        guest_type,
                        yew::AttrValue::from(node_name.clone()),
                        vmid as u32,
                    )
                    .remote(remote_name.clone())
                    .readonly(true)
                    .into(),
                )
            }
            _ => tab_panel,
        };

        tab_panel.into()
    }
}

impl LoadableComponent for FirewallTreeComponent {
    type Properties = super::FirewallTree;
    type Message = Msg;
    type ViewState = ();

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        let tree = create_loading_tree();
        let store = TreeStore::new();
        store.write().update_root_tree(tree);
        store.write().set_view_root(true);

        let link = ctx.link();
        let selection = Selection::new()
            .on_select(link.callback(|_selection: Selection| Msg::SelectionChanged));

        let tab_selection = Selection::new().on_select(link.callback(|_| Msg::TabChanged));

        let (_, context_listener) = ctx
            .link()
            .yew_link()
            .context(ctx.link().callback(|_: RemoteList| Msg::RemoteListChanged))
            .expect("No Remote list context provided");

        store.set_sorter(sort_entries);
        Self {
            store,
            selection,
            tab_selection,
            _context_listener: context_listener,
            filter_text: String::new(),
            scope: Scope::default(),
            available_remotes: Vec::new(),
            available_nodes: Vec::new(),
            options_loading: true,
            load_state: LoadState::default(),
            selected_entry: None,
            tree_collapsed: false,
        }
    }

    fn load(
        &self,
        ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>>>> {
        let link = ctx.link().clone();
        let scope = self.scope.clone();
        let need_remotes = self.available_remotes.is_empty();
        let generation = self.load_state.data_generation;

        Box::pin(async move {
            if need_remotes {
                match load_remotes().await {
                    Ok(remotes) => {
                        link.send_message(Msg::RemotesLoaded(remotes));
                    }
                    Err(err) => {
                        link.send_message(Msg::Error(err));
                    }
                }
            }

            match load_firewall_status(&scope).await {
                Ok(data) => {
                    link.send_message(Msg::DataLoaded { generation, data });
                }
                Err(err) => {
                    link.send_message(Msg::Error(err));
                }
            }

            Ok(())
        })
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::DataLoaded { generation, data } => {
                if !self.load_state.is_data_current(generation) {
                    log::debug!(
                        "Ignoring stale data (generation {} vs current {})",
                        generation,
                        self.load_state.data_generation
                    );
                    return false;
                }

                let tree = build_tree_from_remotes(data);
                self.store.write().set_view_root(false);
                self.store.write().update_root_tree(tree);
                self.load_state.finish_load();
                true
            }
            Msg::RemoteListChanged => true,
            Msg::Reload => {
                let _generation = self.load_state.start_data_load();
                ctx.link().send_reload();
                true
            }
            Msg::FilterChanged(filter) => {
                self.filter_text = filter;
                if self.filter_text.is_empty() {
                    self.store.set_filter(None);
                } else {
                    let filter_text = Rc::new(self.filter_text.to_lowercase());
                    self.store
                        .set_filter(move |entry: &TreeEntry| entry.matches_filter(&filter_text));
                }
                self.clear_selection();
                true
            }
            Msg::ScopeChanged(new_scope) => {
                if self.scope != new_scope {
                    self.handle_scope_change(ctx, new_scope);
                    true
                } else {
                    false
                }
            }
            Msg::RemotesLoaded(remotes) => {
                self.available_remotes = remotes;
                self.options_loading = false;
                true
            }
            Msg::NodesLoaded { generation, nodes } => {
                if !self.load_state.is_nodes_current(generation) {
                    log::debug!(
                        "Ignoring stale nodes (generation {} vs current {})",
                        generation,
                        self.load_state.nodes_generation
                    );
                    return false;
                }
                self.available_nodes = nodes;
                true
            }
            Msg::SelectionChanged => {
                if let Some(key) = self.selection.selected_key() {
                    let read_guard = self.store.read();
                    if let Some(node_ref) = read_guard.lookup_node(&key) {
                        self.selected_entry = Some(node_ref.record().clone());
                    } else {
                        self.selected_entry = None;
                    }
                } else {
                    self.selected_entry = None;
                }
                true
            }
            Msg::TabChanged => true,
            Msg::ToggleTreePanel => {
                self.tree_collapsed = !self.tree_collapsed;
                true
            }
            Msg::Error(err) => {
                log::error!("{}", err);
                ctx.link().show_error(tr!("Error"), err.to_string(), false);
                self.load_state.finish_load();
                true
            }
            Msg::NoOp => false,
        }
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        let mut container = Container::new()
            .class("pwt-content-spacer")
            .class(css::FlexFit)
            .class("pwt-flex-direction-row");

        if !self.tree_collapsed {
            container = container.with_child(self.render_tree_panel(ctx));
        }

        container.with_child(self.render_content_panel(ctx)).into()
    }
}

fn create_remote_combobox(
    ctx: &LoadableComponentContext<FirewallTreeComponent>,
    available_remotes: &[String],
    options_loading: bool,
    current_scope: &Scope,
) -> Html {
    if options_loading {
        return Combobox::new()
            .items(Rc::new(vec![]))
            .placeholder(tr!("Loading..."))
            .disabled(true)
            .key("remote-combobox-loading")
            .on_change(ctx.link().callback(|_: String| Msg::NoOp))
            .into();
    }

    let items: Vec<yew::AttrValue> = available_remotes
        .iter()
        .map(|remote| yew::AttrValue::from(remote.clone()))
        .collect();

    let current_value = current_scope
        .remote_name()
        .map(|s| yew::AttrValue::from(s.to_string()));

    Combobox::new()
        .items(Rc::new(items))
        .default(current_value)
        .placeholder(tr!("All remotes"))
        .disabled(false)
        .key("remote-combobox")
        .on_change(ctx.link().callback(move |value: String| {
            if value.is_empty() {
                Msg::ScopeChanged(Scope::All)
            } else {
                Msg::ScopeChanged(Scope::Remote { name: value })
            }
        }))
        .into()
}

fn create_node_combobox(
    ctx: &LoadableComponentContext<FirewallTreeComponent>,
    available_nodes: &[String],
    options_loading: bool,
    current_scope: &Scope,
) -> Html {
    let selected_remote = current_scope.remote_name();

    let items: Vec<yew::AttrValue> = if selected_remote.is_some() {
        available_nodes
            .iter()
            .map(|node| yew::AttrValue::from(node.clone()))
            .collect()
    } else {
        Vec::new()
    };

    let current_value = current_scope
        .node_name()
        .map(|s| yew::AttrValue::from(s.to_string()));

    let has_nodes = !available_nodes.is_empty();
    let is_enabled = selected_remote.is_some() && !options_loading && has_nodes;
    let key = format!("node-combobox-{:?}", selected_remote);

    let selected_remote_owned = selected_remote.map(String::from);

    Combobox::new()
        .items(Rc::new(items))
        .default(current_value)
        .placeholder(tr!("All nodes"))
        .disabled(!is_enabled)
        .key(key)
        .on_change(ctx.link().callback(move |value: String| {
            if value.is_empty() {
                if let Some(ref remote) = selected_remote_owned {
                    Msg::ScopeChanged(Scope::Remote {
                        name: remote.clone(),
                    })
                } else {
                    Msg::ScopeChanged(Scope::All)
                }
            } else if let Some(ref remote) = selected_remote_owned {
                Msg::ScopeChanged(Scope::Node {
                    remote: remote.clone(),
                    name: value,
                })
            } else {
                Msg::ScopeChanged(Scope::All)
            }
        }))
        .into()
}

async fn load_firewall_status(
    scope: &Scope,
) -> Result<Vec<pdm_api_types::firewall::RemoteFirewallStatus>, FirewallError> {
    match scope {
        Scope::All => crate::pdm_client()
            .pve_get_firewall_status()
            .await
            .map_err(|e| FirewallError::StatusLoadFailed {
                scope: scope.clone(),
                message: e.to_string(),
            }),
        Scope::Remote { name } => {
            let remote_status = crate::pdm_client()
                .pve_cluster_firewall_status(name)
                .await
                .map_err(|e| FirewallError::StatusLoadFailed {
                    scope: scope.clone(),
                    message: e.to_string(),
                })?;
            Ok(vec![remote_status])
        }
        Scope::Node { remote, name } => {
            let node_status = crate::pdm_client()
                .pve_node_firewall_status(remote, name)
                .await
                .map_err(|e| FirewallError::StatusLoadFailed {
                    scope: scope.clone(),
                    message: e.to_string(),
                })?;

            let remote_status = pdm_api_types::firewall::RemoteFirewallStatus {
                remote: remote.clone(),
                status: None,
                nodes: vec![node_status],
            };
            Ok(vec![remote_status])
        }
    }
}

async fn load_remotes() -> Result<Vec<String>, FirewallError> {
    let remotes = crate::pdm_client()
        .list_remotes()
        .await
        .map_err(|e| FirewallError::RemoteListLoadFailed(e.to_string()))?;

    Ok(remotes
        .into_iter()
        .filter(|r| r.ty == RemoteType::Pve)
        .map(|r| r.id)
        .collect())
}

async fn load_nodes_for_remote(remote: String) -> Result<(String, Vec<String>), FirewallError> {
    let resources = crate::pdm_client()
        .pve_cluster_resources(&remote, Some(pdm_client::types::ClusterResourceKind::Node))
        .await
        .map_err(|e| FirewallError::NodesLoadFailed {
            remote: remote.clone(),
            message: e.to_string(),
        })?;

    let mut nodes: Vec<String> = resources
        .into_iter()
        .filter_map(|resource| {
            if let pdm_api_types::resource::PveResource::Node(node) = resource {
                Some(node.node)
            } else {
                None
            }
        })
        .collect();

    nodes.sort();

    Ok((remote, nodes))
}
