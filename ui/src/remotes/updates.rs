use std::cmp::Ordering;
use std::ops::Deref;
use std::pin::Pin;
use std::rc::Rc;

use futures::Future;
use yew::virtual_dom::{Key, VComp, VNode};
use yew::{html, Html, Properties};

use pdm_api_types::remote_updates::{
    NodeUpdateStatus, NodeUpdateSummary, RemoteUpdateStatus, UpdateSummary,
};
use pdm_api_types::remotes::RemoteType;
use pwt::css::{AlignItems, FlexFit, TextAlign};
use pwt::widget::data_table::{DataTableCellRenderArgs, DataTableCellRenderer};

use proxmox_yew_comp::{
    AptPackageManager, LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
};
use pwt::props::{CssBorderBuilder, CssPaddingBuilder, WidgetStyleBuilder};
use pwt::widget::{Button, Container, Panel};
use pwt::{
    css,
    css::FontColor,
    props::{ContainerBuilder, ExtractPrimaryKey, WidgetBuilder},
    state::{Selection, SlabTree, TreeStore},
    tr,
    widget::{
        data_table::{DataTable, DataTableColumn, DataTableHeader},
        Column, Fa, Row,
    },
};

use crate::{get_deep_url, get_deep_url_low_level, pdm_client};

#[derive(PartialEq, Properties)]
pub struct UpdateTree {}

impl UpdateTree {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

impl From<UpdateTree> for VNode {
    fn from(value: UpdateTree) -> Self {
        let comp = VComp::new::<LoadableComponentMaster<UpdateTreeComponent>>(Rc::new(value), None);
        VNode::from(comp)
    }
}

#[derive(Clone, PartialEq, Debug)]
struct RemoteEntry {
    remote: String,
    ty: RemoteType,
    number_of_failed_nodes: u32,
    number_of_nodes: u32,
    number_of_updatable_nodes: u32,
    poll_status: RemoteUpdateStatus,
}

#[derive(Clone, PartialEq, Debug)]
struct NodeEntry {
    remote: String,
    node: String,
    ty: RemoteType,
    summary: NodeUpdateSummary,
    flat: bool,
}

#[derive(Clone, PartialEq, Debug)]
enum UpdateTreeEntry {
    Root,
    Remote(RemoteEntry),
    Node(NodeEntry),
}

impl UpdateTreeEntry {
    fn name(&self) -> &str {
        match &self {
            Self::Root => "",
            Self::Remote(data) => &data.remote,
            Self::Node(data) => {
                if data.flat {
                    &data.remote
                } else {
                    &data.node
                }
            }
        }
    }
}

impl ExtractPrimaryKey for UpdateTreeEntry {
    fn extract_key(&self) -> yew::virtual_dom::Key {
        Key::from(match self {
            UpdateTreeEntry::Root => "/".to_string(),
            UpdateTreeEntry::Remote(data) => format!("/{}", data.remote),
            UpdateTreeEntry::Node(data) => format!("/{}/{}", data.remote, data.node),
        })
    }
}

enum RemoteUpdateTreeMsg {
    LoadFinished(UpdateSummary),
    KeySelected(Option<Key>),
    RefreshAll,
}

struct UpdateTreeComponent {
    store: TreeStore<UpdateTreeEntry>,
    selection: Selection,
    selected_entry: Option<UpdateTreeEntry>,
}

fn default_sorter(a: &UpdateTreeEntry, b: &UpdateTreeEntry) -> Ordering {
    a.name().cmp(b.name())
}

impl UpdateTreeComponent {
    fn columns(
        _ctx: &LoadableComponentContext<Self>,
        store: TreeStore<UpdateTreeEntry>,
    ) -> Rc<Vec<DataTableHeader<UpdateTreeEntry>>> {
        Rc::new(vec![
            DataTableColumn::new(tr!("Name"))
                .tree_column(store)
                .flex(1)
                .render(|entry: &UpdateTreeEntry| {
                    let icon = match entry {
                        UpdateTreeEntry::Remote(_) => Some("server"),
                        UpdateTreeEntry::Node(_) => Some("building"),
                        _ => None,
                    };

                    Row::new()
                        .class(css::AlignItems::Baseline)
                        .gap(2)
                        .with_optional_child(icon.map(|icon| Fa::new(icon)))
                        .with_child(entry.name())
                        .into()
                })
                .sorter(default_sorter)
                .into(),
            DataTableColumn::new(tr!("Status"))
                .flex(3)
                .render_cell(DataTableCellRenderer::new(
                    move |args: &mut DataTableCellRenderArgs<UpdateTreeEntry>| match args.record() {
                        UpdateTreeEntry::Root => {
                            html!()
                        }
                        UpdateTreeEntry::Remote(remote_info) => {
                            render_remote_summary(remote_info, args.is_expanded()).into()
                        }
                        UpdateTreeEntry::Node(info) => render_node_info(info).into(),
                    },
                ))
                .into(),
        ])
    }
}

fn build_store_from_response(update_summary: UpdateSummary) -> SlabTree<UpdateTreeEntry> {
    let mut tree = SlabTree::new();

    let mut root = tree.set_root(UpdateTreeEntry::Root);
    root.set_expanded(true);

    for (remote_name, remote_summary) in update_summary.remotes.deref() {
        if remote_summary.nodes.len() == 1 {
            if let Some((node_name, node_summary)) = remote_summary.nodes.iter().take(1).next() {
                root.append(UpdateTreeEntry::Node(NodeEntry {
                    remote: remote_name.clone(),
                    node: node_name.clone(),
                    ty: remote_summary.remote_type,
                    summary: node_summary.clone(),
                    flat: true,
                }));

                continue;
            }
        }

        let mut remote_entry = root.append(UpdateTreeEntry::Remote(RemoteEntry {
            remote: remote_name.clone(),
            ty: remote_summary.remote_type,
            number_of_nodes: 0,
            number_of_updatable_nodes: 0,
            number_of_failed_nodes: 0,
            poll_status: remote_summary.status.clone(),
        }));
        remote_entry.set_expanded(false);

        let number_of_nodes = remote_summary.nodes.len();
        let mut number_of_updatable_nodes = 0;
        let mut number_of_failed_nodes = 0;

        for (node_name, node_summary) in remote_summary.nodes.deref() {
            match node_summary.status {
                NodeUpdateStatus::Success => {
                    if node_summary.number_of_updates > 0 {
                        number_of_updatable_nodes += 1;
                    }
                }
                NodeUpdateStatus::Error => {
                    number_of_failed_nodes += 1;
                }
            }

            remote_entry.append(UpdateTreeEntry::Node(NodeEntry {
                remote: remote_name.clone(),
                node: node_name.clone(),
                ty: remote_summary.remote_type,
                summary: node_summary.clone(),
                flat: false,
            }));
        }

        if let UpdateTreeEntry::Remote(info) = remote_entry.record_mut() {
            info.number_of_updatable_nodes = number_of_updatable_nodes;
            info.number_of_nodes = number_of_nodes as u32;
            info.number_of_failed_nodes = number_of_failed_nodes as u32;
        }
    }

    tree
}

impl LoadableComponent for UpdateTreeComponent {
    type Properties = UpdateTree;
    type Message = RemoteUpdateTreeMsg;
    type ViewState = ();

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        let link = ctx.link();

        let store = TreeStore::new().view_root(false);
        store.set_sorter(default_sorter);

        link.repeated_load(5000);

        let selection = Selection::new().on_select(link.callback(|selection: Selection| {
            RemoteUpdateTreeMsg::KeySelected(selection.selected_key())
        }));

        Self {
            store: store.clone(),
            selection,
            selected_entry: None,
        }
    }

    fn load(
        &self,
        ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>>>> {
        let link = ctx.link().clone();

        Box::pin(async move {
            let client = pdm_client();

            let updates = client.remote_update_summary().await?;
            link.send_message(Self::Message::LoadFinished(updates));

            Ok(())
        })
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Self::Message::LoadFinished(updates) => {
                let data = build_store_from_response(updates);
                self.store.write().update_root_tree(data);
                self.store.set_sorter(default_sorter);

                return true;
            }
            Self::Message::KeySelected(key) => {
                if let Some(key) = key {
                    let read_guard = self.store.read();
                    let node_ref = read_guard.lookup_node(&key).unwrap();
                    let record = node_ref.record();

                    self.selected_entry = Some(record.clone());

                    return true;
                }
            }
            Self::Message::RefreshAll => {
                let link = ctx.link();

                link.clone().spawn(async move {
                    let client = pdm_client();

                    match client.refresh_remote_update_summary().await {
                        Ok(upid) => {
                            link.show_task_progres(upid.to_string());
                        }
                        Err(err) => {
                            link.show_error(tr!("Could not refresh update status."), err, false);
                        }
                    }
                });
            }
        }

        false
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> yew::Html {
        Container::new()
            .class("pwt-content-spacer")
            .class(FlexFit)
            .class("pwt-flex-direction-row")
            .with_child(self.render_update_tree_panel(ctx))
            .with_child(self.render_update_list_panel(ctx))
            .into()
    }
}

impl UpdateTreeComponent {
    fn render_update_tree_panel(&self, ctx: &LoadableComponentContext<Self>) -> Panel {
        let table = DataTable::new(Self::columns(ctx, self.store.clone()), self.store.clone())
            .selection(self.selection.clone())
            .striped(false)
            .borderless(true)
            .show_header(false)
            .class(css::FlexFit);

        let refresh_all_button = Button::new(tr!("Refresh all")).on_activate({
            let link = ctx.link().clone();
            move |_| {
                link.send_message(RemoteUpdateTreeMsg::RefreshAll);
            }
        });

        let title: Html = Row::new()
            .gap(2)
            .class(AlignItems::Baseline)
            .with_child(Fa::new("refresh"))
            .with_child(tr!("Remote System Updates"))
            .into();

        Panel::new()
            .min_width(500)
            .title(title)
            .with_tool(refresh_all_button)
            .style("flex", "1 1 0")
            .class(FlexFit)
            .border(true)
            .with_child(table)
    }

    fn render_update_list_panel(&self, ctx: &LoadableComponentContext<Self>) -> Panel {
        match &self.selected_entry {
            Some(UpdateTreeEntry::Node(NodeEntry {
                remote, node, ty, ..
            })) => {
                let title: Html = Row::new()
                    .gap(2)
                    .class(AlignItems::Baseline)
                    .with_child(Fa::new("list"))
                    .with_child(tr!("Update List - {} ({})", remote, node))
                    .into();

                let base_url = format!("/{ty}/remotes/{remote}/nodes/{node}/apt",);
                let task_base_url = format!("/{ty}/remotes/{remote}/tasks");

                let apt = AptPackageManager::new()
                    .base_url(base_url)
                    .task_base_url(task_base_url)
                    .enable_upgrade(true)
                    .on_upgrade({
                        let remote = remote.clone();
                        let link = ctx.link().clone();
                        let remote = remote.clone();
                        let node = node.clone();
                        let ty = *ty;

                        move |_| match ty {
                            RemoteType::Pve => {
                                let id = format!("node/{node}::apt");
                                if let Some(url) = get_deep_url(link.yew_link(), &remote, None, &id)
                                {
                                    let _ = gloo_utils::window().open_with_url(&url.href());
                                }
                            }
                            RemoteType::Pbs => {
                                let hash = "#pbsServerAdministration:updates";
                                if let Some(url) =
                                    get_deep_url_low_level(link.yew_link(), &remote, None, &hash)
                                {
                                    let _ = gloo_utils::window().open_with_url(&url.href());
                                }
                            }
                        }
                    });

                Panel::new()
                    .class(FlexFit)
                    .title(title)
                    .border(true)
                    .min_width(500)
                    .with_child(apt)
                    .style("flex", "1 1 0")
            }
            _ => {
                let title: Html = Row::new()
                    .gap(2)
                    .class(AlignItems::Baseline)
                    .with_child(Fa::new("list"))
                    .with_child(tr!("Update List"))
                    .into();

                let header = tr!("No node selected");
                let msg = tr!("Select a node to show available updates.");

                let select_node_msg = Column::new()
                    .class(FlexFit)
                    .padding(2)
                    .class(AlignItems::Center)
                    .class(TextAlign::Center)
                    .with_child(html! {<h1 class="pwt-font-headline-medium">{header}</h1>})
                    .with_child(Container::new().with_child(msg));

                Panel::new()
                    .class(FlexFit)
                    .title(title)
                    .border(true)
                    .min_width(500)
                    .with_child(select_node_msg)
                    .style("flex", "1 1 0")
            }
        }
    }
}

fn render_remote_summary(entry: &RemoteEntry, expanded: bool) -> Row {
    let mut row = Row::new().class(css::AlignItems::Baseline).gap(2);
    match entry.poll_status {
        RemoteUpdateStatus::Success => {
            if !expanded {
                let up_to_date_nodes = entry.number_of_nodes
                    - entry.number_of_updatable_nodes
                    - entry.number_of_failed_nodes;

                let text = if entry.number_of_nodes == up_to_date_nodes {
                    row = row.with_child(render_remote_summary_icon(RemoteSummaryIcon::UpToDate));
                    tr!("All nodes up-to-date")
                } else if entry.number_of_updatable_nodes > 0 {
                    row = row.with_child(render_remote_summary_icon(RemoteSummaryIcon::Updatable));

                    if entry.number_of_failed_nodes > 0 {
                        row = row.with_child(render_remote_summary_icon(RemoteSummaryIcon::Error));
                        // NOTE: This 'summary' line is only shown for remotes with multiple nodes,
                        // so we don't really have to consider the singular form of 'x out of y
                        // nodes'
                        tr!("Some nodes have pending updates, some nodes unavailable")
                    } else {
                        tr!("Some nodes have pending updates")
                    }
                } else if entry.number_of_failed_nodes > 0 {
                    row = row.with_child(render_remote_summary_icon(RemoteSummaryIcon::Error));
                    tr!("Some nodes unavailable")
                } else {
                    String::new()
                };

                row = row.with_child(text);
            }
        }
        RemoteUpdateStatus::Error => {
            row = row.with_child(render_remote_summary_icon(RemoteSummaryIcon::Error));
            row = row.with_child(tr!("Could not connect to remote"));
        }
        RemoteUpdateStatus::Unknown => {
            row = row.with_child(render_remote_summary_icon(RemoteSummaryIcon::Unknown));
            row = row.with_child(tr!("Update status unknown"));
        }
    }

    row
}

fn render_node_info(entry: &NodeEntry) -> Row {
    let (icon, text) = if entry.summary.status == NodeUpdateStatus::Error {
        let icon = render_remote_summary_icon(RemoteSummaryIcon::Error);
        let text = if let Some(status) = &entry.summary.status_message {
            tr!("Failed to retrieve update status: {}", status)
        } else {
            tr!("Unknown error")
        };

        (icon, text)
    } else if entry.summary.number_of_updates > 0 {
        (
            render_remote_summary_icon(RemoteSummaryIcon::Updatable),
            tr!("One update pending" | "{n} updates pending" % entry.summary.number_of_updates),
        )
    } else {
        (
            render_remote_summary_icon(RemoteSummaryIcon::UpToDate),
            tr!("Up-to-date"),
        )
    };

    Row::new()
        .class(css::AlignItems::Baseline)
        .gap(2)
        .with_child(icon)
        .with_child(text)
}

enum RemoteSummaryIcon {
    UpToDate,
    Updatable,
    Error,
    Unknown,
}

fn render_remote_summary_icon(icon: RemoteSummaryIcon) -> Fa {
    let (icon_class, icon_scheme) = match icon {
        RemoteSummaryIcon::UpToDate => ("check", FontColor::Success),
        RemoteSummaryIcon::Error => ("times-circle", FontColor::Error),
        RemoteSummaryIcon::Updatable => ("refresh", FontColor::Primary),
        RemoteSummaryIcon::Unknown => ("question-circle-o", FontColor::Primary),
    };

    Fa::new(icon_class).class(icon_scheme)
}
