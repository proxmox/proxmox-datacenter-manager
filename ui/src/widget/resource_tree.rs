use std::{cmp::Ordering, rc::Rc};

use anyhow::Error;
use gloo_timers::callback::Timeout;
use serde_json::json;
use web_sys::window;
use yew::{virtual_dom::Key, Component};

use pwt::{
    css::{FlexFit, FontColor},
    prelude::*,
    props::ExtractPrimaryKey,
    state::{Selection, TreeStore},
    widget::{
        data_table::{DataTable, DataTableColumn, DataTableHeader},
        ActionIcon, Column, Container, Fa, Panel, Progress, Row, Tooltip,
    },
};
use pwt_macros::{builder, widget};

use proxmox_yew_comp::{http_get, Status};

use pdm_api_types::resource::{RemoteResources, Resource};

use crate::{
    get_deep_url,
    renderer::{render_resource_name, render_status_icon},
    RemoteList,
};

const REFRESH_TIME_S: u32 = 60;
const INPUT_BUFFER_MS: u32 = 500;

#[widget(comp=PdmResourceTree, @element)]
#[derive(Properties, Clone, PartialEq)]
#[builder]
/// A grid with a tree representation of all available resources (Remotes, Nodes, Guests,..)
pub struct ResourceTree {
    #[prop_or_default]
    #[builder]
    /// If given, passing the term to the backend where the filtering happens before the load
    pub search_term: String,

    #[prop_or_default]
    #[builder]
    /// If this is true, we wait with the load until we have a search term
    pub search_only: bool,
}

impl ResourceTree {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

impl Default for ResourceTree {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, PartialEq)]
enum PdmTreeEntry {
    Root,
    Resource(String, Resource),
    Remote(String, Option<String>),
}

impl ExtractPrimaryKey for PdmTreeEntry {
    fn extract_key(&self) -> Key {
        match self {
            PdmTreeEntry::Root => Key::from("__root__"),
            PdmTreeEntry::Resource(_, resource) => Key::from(resource.global_id()),
            PdmTreeEntry::Remote(remote, _) => Key::from(remote.as_str()),
        }
    }
}

async fn load_resources(search_term: String) -> Result<Vec<RemoteResources>, Error> {
    let mut params = json!({ "max-age": REFRESH_TIME_S });
    if !search_term.is_empty() {
        params["search"] = search_term.into();
    }
    http_get("/resources/list", Some(params)).await
}

pub enum Msg {
    Load,
    LoadResult(Result<Vec<RemoteResources>, Error>),
    RemoteListChanged(RemoteList),
}

pub struct PdmResourceTree {
    last_error: Option<Error>,
    loading: bool,
    store: TreeStore<PdmTreeEntry>,
    remote_list: RemoteList,
    _context_listener: ContextHandle<RemoteList>,
    selection: Selection,
    _load_timeout: Option<Timeout>,
}

impl PdmResourceTree {}

impl Component for PdmResourceTree {
    type Message = Msg;
    type Properties = ResourceTree;

    fn create(ctx: &yew::Context<Self>) -> Self {
        let props = ctx.props();
        let (list, _context_listener) = ctx
            .link()
            .context(ctx.link().callback(Msg::RemoteListChanged))
            .expect("No Remote list context provided");

        if !props.search_only || !props.search_term.is_empty() {
            ctx.link().clone().send_message(Msg::Load);
        }

        let store = TreeStore::new().view_root(false);
        let selection = Selection::new().on_select({
            let store = store.clone();
            let link = ctx.link().clone();
            move |selection: Selection| {
                let store = store.read();
                let root = store.root().unwrap();

                if let Some(key) = selection.selected_key() {
                    if let Some(node) = root.find_node_by_key(&key) {
                        match node.record() {
                            PdmTreeEntry::Resource(remote, resource) => {
                                crate::navigate_to(&link, remote, Some(resource));
                            }
                            PdmTreeEntry::Remote(remote, _) => {
                                crate::navigate_to(&link, remote, None);
                            }
                            _ => {}
                        }
                    }
                }
            }
        });
        Self {
            loading: false,
            last_error: None,
            store,
            remote_list: list,
            _context_listener,
            selection,
            _load_timeout: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Load => {
                let props = ctx.props();
                let link = ctx.link().clone();
                let search_term = props.search_term.clone();
                if props.search_only && !search_term.is_empty() {
                    self._load_timeout = Some(Timeout::new(INPUT_BUFFER_MS, move || {
                        link.send_future(async move {
                            Msg::LoadResult(load_resources(search_term).await)
                        });
                    }));
                    self.loading = true;
                }
                true
            }
            Msg::LoadResult(res) => {
                match res {
                    Ok(result) => {
                        self.last_error = None;
                        let mut store = self.store.write();
                        let mut root = store.set_root(PdmTreeEntry::Root);
                        for res in result.into_iter() {
                            let mut node =
                                root.append(PdmTreeEntry::Remote(res.remote.clone(), res.error));
                            node.set_expanded(true);
                            for entry in res.resources.into_iter() {
                                if let Resource::PbsNode(_) = entry {
                                    continue;
                                }

                                node.append(PdmTreeEntry::Resource(res.remote.clone(), entry));
                            }
                        }
                        store.sort_by(true, |a, b| match (a, b) {
                            (PdmTreeEntry::Root, PdmTreeEntry::Root) => Ordering::Equal,
                            (PdmTreeEntry::Root, _) => Ordering::Less,
                            (_, PdmTreeEntry::Root) => Ordering::Greater,
                            (PdmTreeEntry::Remote(a, _), PdmTreeEntry::Remote(b, _)) => a.cmp(b),
                            (PdmTreeEntry::Remote(_, _), _) => Ordering::Less,
                            (_, PdmTreeEntry::Remote(_, _)) => Ordering::Greater,
                            (PdmTreeEntry::Resource(_, a), PdmTreeEntry::Resource(_, b)) => {
                                a.id().cmp(&b.id())
                            }
                        });
                    }
                    Err(err) => self.last_error = Some(err),
                }
                self.loading = false;
                true
            }
            Msg::RemoteListChanged(list) => {
                let reload = self.remote_list.len() != list.len();
                self.remote_list = list;
                if reload && !self.remote_list.is_empty() {
                    ctx.link().send_message(Msg::Load);
                }
                true
            }
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();
        if props.search_term != old_props.search_term {
            if !props.search_only || !props.search_term.is_empty() {
                ctx.link().clone().send_message(Msg::Load);
            } else if props.search_term.is_empty() {
                // clear grid
                ctx.link()
                    .clone()
                    .send_message(Msg::LoadResult(Ok(Vec::new())));
            }
        }

        true
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();
        let table = DataTable::new(columns(&ctx.link(), self.store.clone()), self.store.clone())
            .selection(self.selection.clone())
            .class(FlexFit)
            .hover(true)
            .borderless(true);

        let has_data = self
            .store
            .read()
            .root()
            .map(|root| root.children_count() > 0)
            .unwrap_or_default();

        Panel::new()
            .with_std_props(&props.std_props)
            .listeners(&props.listeners)
            .with_child(
                Column::new()
                    .class(FlexFit)
                    // FIXME: use streaming with some kind of indicator instead of a loadmask
                    .with_child(table)
                    .with_optional_child(self.loading.then_some(Progress::new()))
                    .with_optional_child(
                        (!has_data && !self.loading).then_some(
                            Row::new()
                                .padding(4)
                                .gap(2)
                                .with_child(Fa::new("search").class(FontColor::NeutralAlt).large())
                                .with_child(tr!("No data")),
                        ),
                    )
                    .with_optional_child(self.last_error.as_deref().map(|err| {
                        Row::new()
                            .border_top(true)
                            .padding(4)
                            .gap(2)
                            .with_child(Status::Error.to_fa_icon().large())
                            .with_child(err.to_string())
                    })),
            )
            .into()
    }
}

fn columns(
    link: &html::Scope<PdmResourceTree>,
    store: TreeStore<PdmTreeEntry>,
) -> Rc<Vec<DataTableHeader<PdmTreeEntry>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("ID"))
            .tree_column(store)
            .flex(1)
            .render(|item: &PdmTreeEntry| {
                let (icon, text, tooltip) = match item {
                    PdmTreeEntry::Root => (
                        Container::new().with_child(Fa::new("server").fixed_width()),
                        String::from("root"),
                        None,
                    ),
                    PdmTreeEntry::Resource(_, resource) => (
                        render_status_icon(resource),
                        render_resource_name(resource, true),
                        None,
                    ),
                    PdmTreeEntry::Remote(remote, err) => (
                        Container::new()
                            .class("pdm-type-icon")
                            .with_child(Fa::new("server").fixed_width())
                            .with_optional_child(
                                err.is_some().then_some(
                                    Status::Error
                                        .to_fa_icon()
                                        .fixed_width()
                                        .class("status-icon"),
                                ),
                            ),
                        remote.clone(),
                        err.as_ref().map(|err| err.to_string()),
                    ),
                };
                Tooltip::new(Row::new().gap(4).with_child(icon).with_child(text))
                    .tip(tooltip)
                    .into()
            })
            .into(),
        DataTableColumn::new(tr!("Node"))
            .flex(2)
            .render(|item: &PdmTreeEntry| {
                match item {
                    PdmTreeEntry::Root => "",
                    PdmTreeEntry::Resource(_, resource) => match resource {
                        Resource::PveStorage(r) => &r.node,
                        Resource::PveQemu(r) => &r.node,
                        Resource::PveLxc(r) => &r.node,
                        _ => "",
                    },
                    PdmTreeEntry::Remote(_, _) => "",
                }
                .into()
            })
            .into(),
        DataTableColumn::new("")
            .width("50px")
            .justify("right")
            .render({
                let link = link.clone();
                move |item: &PdmTreeEntry| {
                    let (remote, id) = match item {
                        PdmTreeEntry::Root => return html! {},
                        PdmTreeEntry::Resource(remote_id, resource) => (remote_id, resource.id()),
                        PdmTreeEntry::Remote(remote_id, _) => (remote_id, String::new()),
                    };

                    match get_deep_url(&link, remote, &id) {
                        Some(url) => ActionIcon::new("fa fa-chevron-right")
                            .on_activate(move |()| {
                                let _ = window().unwrap().open_with_url(&url.href());
                            })
                            .into(),
                        None => html! {},
                    }
                }
            })
            .into(),
    ])
}
