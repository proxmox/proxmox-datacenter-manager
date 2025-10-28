use futures::Future;
use std::cmp::Ordering;
use std::pin::Pin;
use std::rc::Rc;

use yew::virtual_dom::{Key, VComp, VNode};
use yew::{html, ContextHandle, Html, Properties};

use pdm_api_types::resource::{PveNetworkResource, RemoteResources, ResourceType, SdnStatus};
use pdm_client::types::{ClusterResourceNetworkType, Resource};
use proxmox_yew_comp::{LoadableComponent, LoadableComponentContext, LoadableComponentMaster};
use pwt::props::EventSubscriber;
use pwt::widget::{ActionIcon, Button, Toolbar};
use pwt::{
    css,
    css::FontColor,
    props::{ContainerBuilder, ExtractPrimaryKey, WidgetBuilder},
    state::{Selection, SlabTree, TreeStore},
    tr,
    widget::{
        data_table::{DataTable, DataTableColumn, DataTableHeader},
        error_message, Column, Fa, Row,
    },
};

use crate::{get_deep_url, pdm_client, RemoteList};

#[derive(PartialEq, Properties)]
pub struct ZoneTree {}

impl ZoneTree {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

impl From<ZoneTree> for VNode {
    fn from(value: ZoneTree) -> Self {
        let comp = VComp::new::<LoadableComponentMaster<ZoneTreeComponent>>(Rc::new(value), None);
        VNode::from(comp)
    }
}

#[derive(Clone, PartialEq, Debug)]
struct NetworkData {
    remote: String,
    node: String,
    name: String,
    network_type: ClusterResourceNetworkType,
    status: SdnStatus,
    legacy: bool,
}

#[derive(Clone, PartialEq, Debug)]
enum ZoneTreeEntry {
    Root,
    Remote(String),
    Node(String, String),
    NetworkResource(NetworkData),
}

impl ZoneTreeEntry {
    fn from_network_resource(remote: String, value: PveNetworkResource) -> Self {
        let network_type = value.network_type();

        match value {
            PveNetworkResource::Zone(zone) => Self::NetworkResource(NetworkData {
                remote: remote,
                node: zone.node,
                name: zone.network,
                status: zone.status,
                network_type,
                legacy: zone.legacy,
            }),
            PveNetworkResource::Fabric(fabric) => Self::NetworkResource(NetworkData {
                remote: remote,
                node: fabric.node,
                name: fabric.network,
                status: fabric.status,
                network_type,
                legacy: false,
            }),
        }
    }

    fn name(&self) -> &str {
        match &self {
            Self::Root => "",
            Self::Remote(name) => name,
            Self::Node(_, name) => name,
            Self::NetworkResource(network_resource) => &network_resource.name,
        }
    }
}

impl ExtractPrimaryKey for ZoneTreeEntry {
    fn extract_key(&self) -> yew::virtual_dom::Key {
        Key::from(match self {
            ZoneTreeEntry::Root => "/".to_string(),
            ZoneTreeEntry::Remote(name) => format!("/{name}"),
            ZoneTreeEntry::Node(remote_name, name) => format!("/{remote_name}/{name}"),
            ZoneTreeEntry::NetworkResource(r) => {
                format!("/{}/{}/{}/{}", r.remote, r.node, r.network_type, r.name)
            }
        })
    }
}

pub enum ZoneTreeMsg {
    LoadFinished(Vec<RemoteResources>),
    RemoteListChanged(RemoteList),
    Reload,
}

pub struct ZoneTreeComponent {
    store: TreeStore<ZoneTreeEntry>,
    selection: Selection,
    remote_errors: Vec<String>,
    _context_listener: ContextHandle<RemoteList>,
}

fn default_sorter(a: &ZoneTreeEntry, b: &ZoneTreeEntry) -> Ordering {
    a.name().cmp(b.name())
}

impl ZoneTreeComponent {
    fn columns(
        ctx: &LoadableComponentContext<Self>,
        store: TreeStore<ZoneTreeEntry>,
    ) -> Rc<Vec<DataTableHeader<ZoneTreeEntry>>> {
        let link = ctx.link().clone();

        Rc::new(vec![
            DataTableColumn::new(tr!("Name"))
                .tree_column(store)
                .render(|entry: &ZoneTreeEntry| {
                    let icon = match entry {
                        ZoneTreeEntry::Remote(_) => Some("server"),
                        ZoneTreeEntry::Node(_, _) => Some("building"),
                        ZoneTreeEntry::NetworkResource(r) => match r.network_type {
                            ClusterResourceNetworkType::Fabric => Some("road"),
                            ClusterResourceNetworkType::Zone => Some("th"),
                            ClusterResourceNetworkType::UnknownEnumValue(variant) => {
                                log::warn!("unknown network resource type {variant}");
                                Some("question-circle-o")
                            }
                        },
                        _ => None,
                    };

                    Row::new()
                        .class(css::AlignItems::Baseline)
                        .gap(2)
                        .with_optional_child(icon.map(Fa::new))
                        .with_child(entry.name())
                        .into()
                })
                .sorter(default_sorter)
                .into(),
            DataTableColumn::new(tr!("Status"))
                .render(|entry: &ZoneTreeEntry| {
                    let mut row = Row::new().class(css::AlignItems::Baseline).gap(2);

                    if let ZoneTreeEntry::NetworkResource(r) = entry {
                        row = match r.status {
                            SdnStatus::Available => {
                                row.with_child(Fa::new("check").class(FontColor::Success))
                            }
                            SdnStatus::Pending => {
                                row.with_child(Fa::new("refresh").class(FontColor::Warning))
                            }
                            SdnStatus::Error => {
                                row.with_child(Fa::new("times-circle").class(FontColor::Error))
                            }
                            _ => row,
                        };

                        row = row.with_child(r.status);
                    } else {
                        row = row.with_child("");
                    }

                    row.into()
                })
                .into(),
            DataTableColumn::new(tr!("Actions"))
                .width("80px")
                .justify("right")
                .render(move |entry: &ZoneTreeEntry| {
                    let url = match entry {
                        ZoneTreeEntry::Root | ZoneTreeEntry::Node(_, _) => None,
                        ZoneTreeEntry::Remote(remote) => {
                            // TODO: do not hardcode this here.
                            let hash = "#v1:0:18:4:::::::53";
                            crate::get_deep_url_low_level(link.yew_link(), remote, None, hash)
                        }
                        ZoneTreeEntry::NetworkResource(network_resource) => {
                            if network_resource.legacy {
                                let id = format!(
                                    "sdn/{}/{}",
                                    network_resource.node, network_resource.name
                                );
                                get_deep_url(link.yew_link(), &network_resource.remote, None, &id)
                            } else {
                                let id = format!(
                                    "network/{}/{}/{}",
                                    network_resource.node,
                                    network_resource.network_type,
                                    network_resource.name
                                );
                                get_deep_url(link.yew_link(), &network_resource.remote, None, &id)
                            }
                        }
                    };

                    match url {
                        Some(url) => ActionIcon::new("fa fa-external-link")
                            .on_activate(move |_| {
                                let _ = web_sys::window().unwrap().open_with_url(&url.href());
                            })
                            .into(),
                        None => html! {},
                    }
                })
                .into(),
        ])
    }
}

fn build_store_from_response(
    remote_resources: Vec<RemoteResources>,
) -> (SlabTree<ZoneTreeEntry>, Vec<String>) {
    let mut tree = SlabTree::new();

    let mut root = tree.set_root(ZoneTreeEntry::Root);
    root.set_expanded(true);

    let mut remote_errors = Vec::new();

    for resources in remote_resources {
        if let Some(error) = resources.error {
            remote_errors.push(format!(
                "could not fetch resources from remote {}: {error}",
                resources.remote,
            ));
            continue;
        }

        let mut remote = root.append(ZoneTreeEntry::Remote(resources.remote.clone()));
        remote.set_expanded(true);

        for resource in resources.resources {
            let Resource::PveNetwork(resource) = resource else {
                continue;
            };

            let node_entry = remote.children_mut().find(|entry| {
                if let ZoneTreeEntry::Node(_, name) = entry.record() {
                    return name == resource.node();
                }

                false
            });

            let node_name = resource.node().to_string();
            let entry = ZoneTreeEntry::from_network_resource(resources.remote.clone(), resource);

            match node_entry {
                Some(mut node_entry) => {
                    node_entry.append(entry);
                }
                None => {
                    let mut node_entry =
                        remote.append(ZoneTreeEntry::Node(resources.remote.clone(), node_name));

                    node_entry.set_expanded(true);

                    node_entry.append(entry);
                }
            };
        }
    }

    (tree, remote_errors)
}

impl LoadableComponent for ZoneTreeComponent {
    type Properties = ZoneTree;
    type Message = ZoneTreeMsg;
    type ViewState = ();

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        let store = TreeStore::new().view_root(false);
        store.set_sorter(default_sorter);

        let selection = Selection::new();

        let (_, _context_listener) = ctx
            .link()
            .yew_link()
            .context(ctx.link().callback(Self::Message::RemoteListChanged))
            .expect("No Remote list context provided");

        Self {
            store: store.clone(),
            selection,
            remote_errors: Vec::new(),
            _context_listener,
        }
    }

    fn load(
        &self,
        ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>>>> {
        let link = ctx.link().clone();

        Box::pin(async move {
            let client = pdm_client();
            let remote_resources = client
                .resources_by_type(None, ResourceType::PveNetwork, None)
                .await?;
            link.send_message(Self::Message::LoadFinished(remote_resources));

            Ok(())
        })
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Self::Message::LoadFinished(remote_resources) => {
                let (data, remote_errors) = build_store_from_response(remote_resources);
                self.store.write().update_root_tree(data);
                self.store.set_sorter(default_sorter);

                self.remote_errors = remote_errors;

                return true;
            }
            Self::Message::RemoteListChanged(_list) => {
                return true;
            }
            Self::Message::Reload => {
                ctx.link().send_reload();
            }
        }

        false
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        let on_refresh = ctx.link().callback(|_| ZoneTreeMsg::Reload);

        Some(
            Toolbar::new()
                .class("pwt-w-100")
                .class("pwt-overflow-hidden")
                .class("pwt-border-bottom")
                .with_flex_spacer()
                .with_child(Button::refresh(ctx.loading()).onclick(on_refresh))
                .into(),
        )
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> yew::Html {
        let table = DataTable::new(Self::columns(ctx, self.store.clone()), self.store.clone())
            .selection(self.selection.clone())
            .striped(false)
            .class(css::FlexFit);

        let mut column = Column::new().class(pwt::css::FlexFit).with_child(table);

        for remote_error in &self.remote_errors {
            column.add_child(error_message(remote_error));
        }

        column.into()
    }
}
