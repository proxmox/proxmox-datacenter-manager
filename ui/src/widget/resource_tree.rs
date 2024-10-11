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
        ActionIcon, Column, Fa, Mask, Panel, Row,
    },
};
use pwt_macros::{builder, widget};

use proxmox_yew_comp::http_get;

use pdm_api_types::resource::{RemoteResources, Resource};

use crate::RemoteList;

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

        Self {
            loading: false,
            last_error: None,
            store: TreeStore::new().view_root(false),
            remote_list: list,
            _context_listener,
            selection: Selection::new(),
            _load_timeout: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Load => {
                let link = ctx.link().clone();
                let search_term = ctx.props().search_term.clone();
                self._load_timeout = Some(Timeout::new(INPUT_BUFFER_MS, move || {
                    link.send_future(
                        async move { Msg::LoadResult(load_resources(search_term).await) },
                    );
                }));
                self.loading = true;
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
        let table = DataTable::new(
            columns(&self.remote_list, self.store.clone()),
            self.store.clone(),
        )
        .selection(self.selection.clone())
        .header_focusable(false)
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
                    .with_child(Mask::new(table).class(FlexFit).visible(self.loading))
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
                            .with_child(
                                Fa::new("exclamation-triangle")
                                    .class(FontColor::Error)
                                    .large(),
                            )
                            .with_child(err.to_string())
                    })),
            )
            .into()
    }
}

fn get_type_and_icon(item: &PdmTreeEntry) -> (&'static str, &'static str) {
    match item {
        PdmTreeEntry::Root => ("root", ""),
        PdmTreeEntry::Resource(_, resource) => match resource {
            Resource::PveStorage(_) => ("pve-storage", "database"),
            Resource::PveQemu(_) => ("qemu", "desktop"),
            Resource::PveLxc(_) => ("lxc", "cubes"),
            Resource::PveNode(_) => ("pve", "building"),
            Resource::PbsNode(_) => ("pbs", "building"),
            Resource::PbsDatastore(_) => ("pbs-datastore", "database"),
        },
        PdmTreeEntry::Remote(_, _) => ("remote", "server"),
    }
}

fn find_remote<'a>(list: &'a RemoteList, id: &str) -> Option<&'a pdm_client::types::Remote> {
    list.iter().find(|remote| remote.id == id)
}
fn columns(
    list: &RemoteList,
    store: TreeStore<PdmTreeEntry>,
) -> Rc<Vec<DataTableHeader<PdmTreeEntry>>> {
    let list = list.clone();

    Rc::new(vec![
        DataTableColumn::new(tr!("ID"))
            .tree_column(store)
            .flex(1)
            .render(|item: &PdmTreeEntry| {
                let (_, icon) = get_type_and_icon(item);
                let icon = Fa::new(icon).class("fa-fw").padding_end(2);

                let text = match item {
                    PdmTreeEntry::Root => String::from("root"),
                    PdmTreeEntry::Resource(_, resource) => resource.id(),
                    PdmTreeEntry::Remote(remote, _) => remote.clone(),
                };
                html! {<>{icon}{text}</>}
            })
            .into(),
        DataTableColumn::new(tr!("Name"))
            .flex(2)
            .render(|item: &PdmTreeEntry| {
                match item {
                    PdmTreeEntry::Root => "",
                    PdmTreeEntry::Resource(_, resource) => resource.name(),
                    PdmTreeEntry::Remote(remote, _) => remote,
                }
                .into()
            })
            .into(),
        DataTableColumn::new(tr!("Status"))
            .justify("center")
            .render(|item: &PdmTreeEntry| match item {
                PdmTreeEntry::Root => html! {},
                PdmTreeEntry::Resource(_, resource) => {
                    let (icon, color) = match &resource {
                        Resource::PveStorage(r) => {
                            if r.status == "available" {
                                ("check-circle", FontColor::Success)
                            } else {
                                ("times-circle", FontColor::Error)
                            }
                        }
                        Resource::PveQemu(r) => match r.status.as_str() {
                            "running" => ("play", FontColor::Success),
                            "stopped" => ("stop", FontColor::InverseSurface),
                            _ => ("question-circle", FontColor::Surface),
                        },
                        Resource::PveLxc(r) => match r.status.as_str() {
                            "running" => ("play", FontColor::Success),
                            "stopped" => ("stop", FontColor::InverseSurface),
                            _ => ("question-circle", FontColor::Surface),
                        },
                        Resource::PveNode(r) => {
                            if r.uptime > 0 {
                                ("check-circle", FontColor::Success)
                            } else {
                                ("times-circle", FontColor::Error)
                            }
                        }
                        Resource::PbsNode(r) => {
                            if r.uptime > 0 {
                                ("check-circle", FontColor::Success)
                            } else {
                                ("times-circle", FontColor::Error)
                            }
                        }
                        Resource::PbsDatastore(_) => ("", FontColor::Primary),
                    };
                    Fa::new(icon).class(color).into()
                }
                PdmTreeEntry::Remote(_, error) => match error {
                    Some(_) => Fa::new("triangle-exclamation")
                        .class(FontColor::Error)
                        .into(),
                    None => Fa::new("check-circle").class(FontColor::Success).into(),
                },
            })
            .into(),
        DataTableColumn::new("")
            .width("50px")
            .justify("right")
            .render(move |item: &PdmTreeEntry| {
                let (remote, id) = match item {
                    PdmTreeEntry::Root => return html! {},
                    PdmTreeEntry::Resource(remote_id, resource) => {
                        (find_remote(&list, remote_id), resource.id())
                    }
                    PdmTreeEntry::Remote(remote_id, _) => {
                        (find_remote(&list, remote_id), String::new())
                    }
                };
                if remote.is_none() {
                    return html! {};
                }
                let remote = remote.unwrap();
                let (default_port, hash) = match remote.ty {
                    pdm_api_types::remotes::RemoteType::Pve => (
                        "8006",
                        if id.is_empty() {
                            id
                        } else {
                            format!("v1::={id}")
                        },
                    ),
                    pdm_api_types::remotes::RemoteType::Pbs => (
                        "8007",
                        if id.is_empty() {
                            id
                        } else {
                            format!("DataStore-{id}")
                        },
                    ),
                };
                let link = remote.nodes.first().and_then(|node| {
                    let url = web_sys::Url::new(&format!("https://{}/", node.hostname));
                    if let Ok(url) = url {
                        if url.port() == "" {
                            url.set_port(default_port);
                        }
                        url.set_hash(&hash);
                        Some(url)
                    } else {
                        None
                    }
                });
                match link {
                    Some(url) => ActionIcon::new("fa fa-chevron-right")
                        .on_activate(move |()| {
                            let _ = window().unwrap().open_with_url(&url.href());
                        })
                        .into(),
                    None => html! {},
                }
            })
            .into(),
    ])
}
