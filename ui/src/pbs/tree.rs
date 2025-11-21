use std::cmp::Ordering;
use std::rc::Rc;

use gloo_utils::window;
use yew::html::Scope;
use yew::virtual_dom::{Key, VComp, VNode};
use yew::{Component, Properties};

use pwt::css::{AlignItems, FlexFit, FontStyle};
use pwt::prelude::*;
use pwt::props::ExtractPrimaryKey;
use pwt::state::{KeyedSlabTree, NavigationContext, NavigationContextExt, Selection, TreeStore};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::Field;
use pwt::widget::{ActionIcon, Button, Column, Container, Fa, Row, Toolbar, Tooltip, Trigger};

use pbs_api_types::DataStoreConfig;

use crate::get_deep_url;
use crate::renderer::render_tree_column;

#[derive(Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum PbsTreeNode {
    Root,
    Datastore(DataStoreConfig),
}

impl ExtractPrimaryKey for PbsTreeNode {
    fn extract_key(&self) -> yew::virtual_dom::Key {
        match self {
            PbsTreeNode::Root => Key::from("__root__"),
            PbsTreeNode::Datastore(datastore) => Key::from(datastore.name.as_str()),
        }
    }
}

#[derive(PartialEq, Properties)]
pub struct PbsTree {
    remote: String,
    resources: Rc<Vec<DataStoreConfig>>,
    loading: bool,
    on_select: Callback<PbsTreeNode>,
    on_reload_click: Callback<()>,
}

impl PbsTree {
    pub fn new(
        remote: String,
        resources: Rc<Vec<DataStoreConfig>>,
        loading: bool,
        on_select: impl Into<Callback<PbsTreeNode>>,
        on_reload_click: impl Into<Callback<()>>,
    ) -> Self {
        Self {
            remote,
            resources,
            loading,
            on_select: on_select.into(),
            on_reload_click: on_reload_click.into(),
        }
    }
}

impl From<PbsTree> for VNode {
    fn from(val: PbsTree) -> Self {
        VComp::new::<PbsTreeComp>(Rc::new(val), None).into()
    }
}

pub enum Msg {
    Filter(String),
    KeySelected(Option<Key>),
    RouteChanged(String),
}

#[doc(hidden)]
pub struct PbsTreeComp {
    columns: Rc<Vec<DataTableHeader<PbsTreeNode>>>,
    filter: String,
    store: TreeStore<PbsTreeNode>,
    view_selection: Selection,
    loaded: bool,
    _nav_handle: ContextHandle<NavigationContext>,
}

impl PbsTreeComp {
    fn load_tree(&mut self, ctx: &yew::Context<Self>) {
        let mut tree = KeyedSlabTree::new();
        let mut root = tree.set_root(PbsTreeNode::Root);

        for datastore in ctx.props().resources.iter() {
            root.append(PbsTreeNode::Datastore(datastore.clone()));
        }

        root.set_expanded(true);
        root.sort_by(true, |a, b| match (a, b) {
            (PbsTreeNode::Root, PbsTreeNode::Root) => Ordering::Equal,
            (PbsTreeNode::Root, _) => Ordering::Less,
            (_, PbsTreeNode::Root) => Ordering::Greater,
            (PbsTreeNode::Datastore(a), PbsTreeNode::Datastore(b)) => a.name.cmp(&b.name),
        });

        if !self.loaded {
            let select_key = self
                .view_selection
                .selected_key()
                .unwrap_or(Key::from("__root__"));
            if let Some(node) = tree.lookup_node(&select_key) {
                self.view_selection.select(select_key);
                ctx.props().on_select.emit(node.record().clone());
            }
        }

        self.store.write().update_root_tree(tree);
        self.loaded = true;
    }
}

impl Component for PbsTreeComp {
    type Message = Msg;
    type Properties = PbsTree;

    fn create(ctx: &yew::Context<Self>) -> Self {
        let mut tree = KeyedSlabTree::new();
        tree.set_root(PbsTreeNode::Root);
        let store = TreeStore::new();
        store.write().update_root_tree(tree);

        let props = ctx.props();
        let view_selection = Selection::new().on_select(
            ctx.link()
                .callback(|selection: Selection| Msg::KeySelected(selection.selected_key())),
        );

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
        Self {
            columns: columns(
                ctx.link().clone(),
                store.clone(),
                props.remote.clone(),
                props.loading,
            ),
            filter: String::new(),
            store,
            view_selection,
            loaded: false,
            _nav_handle,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Filter(filter) => {
                let changed = self.filter != filter;
                self.filter = filter;
                if self.filter.is_empty() {
                    self.store.set_filter(None);
                } else {
                    let text = self.filter.to_lowercase();
                    self.store
                        .set_filter(move |entry: &PbsTreeNode| match entry {
                            PbsTreeNode::Root => true,
                            PbsTreeNode::Datastore(datastore) => {
                                datastore.name.to_lowercase().contains(&text)
                            }
                        });
                }
                changed
            }
            Msg::KeySelected(key) => {
                let key = key.unwrap_or_else(|| Key::from("__root__"));
                let store = self.store.read();
                let root = store.root().unwrap();

                if let Some(node) = root.find_node_by_key(&key) {
                    let record = node.record().clone();
                    let route = match &record {
                        PbsTreeNode::Root => String::new(),
                        PbsTreeNode::Datastore(datastore) => datastore.name.to_string(),
                    };
                    ctx.link().push_relative_route(&route);
                    ctx.props().on_select.emit(record);
                }
                true
            }
            Msg::RouteChanged(path) => {
                let key = if path == "_" || path.is_empty() {
                    Key::from("__root__")
                } else {
                    Key::from(path)
                };
                self.view_selection.select(key);
                true
            }
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        if ctx.props().resources != old_props.resources {
            self.load_tree(ctx);
        }
        true
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let link = ctx.link();

        let nav = DataTable::new(Rc::clone(&self.columns), self.store.clone())
            .selection(self.view_selection.clone())
            .striped(false)
            .borderless(true)
            .hover(true)
            .class(FlexFit)
            .show_header(false);
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
                            .with_child(Fa::new("database"))
                            .with_child(tr!("Datastores")),
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
}

fn columns(
    link: Scope<PbsTreeComp>,
    store: TreeStore<PbsTreeNode>,
    remote: String,
    loading: bool,
) -> Rc<Vec<DataTableHeader<PbsTreeNode>>> {
    let loading = match store.read().root() {
        Some(root) => loading && root.children_count() == 0,
        None => loading,
    };
    let remote_name = remote.clone();
    let tree_column = DataTableColumn::new("Type/ID")
        .flex(1)
        .tree_column(store)
        .render(move |entry: &PbsTreeNode| {
            let (icon, text) = match entry {
                PbsTreeNode::Root if loading => (
                    Container::from_tag("i").class("pwt-loading-icon"),
                    tr!("Querying Remote..."),
                ),
                PbsTreeNode::Root => (
                    Container::new().with_child(Fa::new("building-o")),
                    // TODO: get real node hostname from PBS API.
                    remote_name.clone(),
                ),
                PbsTreeNode::Datastore(datastore) => (
                    Container::new().with_child(Fa::new("database")),
                    datastore.name.clone(),
                ),
            };
            render_tree_column(icon.into(), text).into()
        })
        .into();

    let link_column = DataTableColumn::new("link")
        .render(move |entry: &PbsTreeNode| {
            let local_id = match entry {
                PbsTreeNode::Root => String::new(),
                PbsTreeNode::Datastore(datastore) => datastore.name.clone(),
            };
            Tooltip::new(ActionIcon::new("fa fa-external-link").on_activate({
                let link = link.clone();
                let remote = remote.clone();
                move |_| {
                    // there must be a remote with a connections config if were already here
                    if let Some(url) = get_deep_url(&link, &remote, None, &local_id) {
                        let _ = window().open_with_url(&url.href());
                    }
                }
            }))
            .tip(tr!("Open in PBS UI"))
            .into()
        })
        .into();

    Rc::new(vec![tree_column, link_column])
}
