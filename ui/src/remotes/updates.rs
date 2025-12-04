use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::ops::Deref;
use std::pin::Pin;
use std::rc::Rc;

use futures::Future;
use yew::virtual_dom::{Key, VComp, VNode};
use yew::{html, Html, Properties};

use pdm_api_types::remote_updates::{
    NodeUpdateStatus, NodeUpdateSummary, ProductRepositoryStatus, RemoteUpdateStatus, UpdateSummary,
};
use pdm_api_types::remotes::RemoteType;
use pwt::css::{AlignItems, FlexFit, TextAlign};
use pwt::widget::data_table::{DataTableCellRenderArgs, DataTableCellRenderer};

use proxmox_deb_version;

use proxmox_yew_comp::{
    AptPackageManager, AptRepositories, ExistingProduct, LoadableComponent,
    LoadableComponentContext, LoadableComponentMaster,
};
use pwt::props::{CssBorderBuilder, CssPaddingBuilder, WidgetStyleBuilder};
use pwt::widget::{Button, Container, Panel, Progress, Tooltip};
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

use crate::{
    check_pdm_subscription, get_deep_url, get_deep_url_low_level, pdm_client,
    pdm_subscription_alert,
};

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

#[derive(Clone, PartialEq, Debug, PartialOrd)]
enum MixedVersions {
    None,
    DifferentMajor,
    DifferentMinor,
    DifferentPatch,
}

#[derive(Clone, PartialEq, Debug)]
struct RemoteEntry {
    remote: String,
    ty: RemoteType,
    product_version: Option<String>,
    mixed_versions: MixedVersions,
    number_of_failed_nodes: u32,
    number_of_nodes: u32,
    number_of_updatable_nodes: u32,
    repo_status: ProductRepositoryStatus,
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
    RefreshFinished,
    CheckSubscription,
}

struct UpdateTreeComponent {
    store: TreeStore<UpdateTreeEntry>,
    selection: Selection,
    selected_entry: Option<UpdateTreeEntry>,
    refreshing: bool,
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
                .flex(2)
                .render(|entry: &UpdateTreeEntry| {
                    let icon = match entry {
                        UpdateTreeEntry::Remote(_) => Some("server"),
                        UpdateTreeEntry::Node(node_entry) => {
                            if node_entry.ty == RemoteType::Pbs {
                                Some("building-o")
                            } else {
                                Some("building")
                            }
                        }
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
            DataTableColumn::new(tr!("Version"))
                .flex(1)
                .render_cell(DataTableCellRenderer::new(
                    move |args: &mut DataTableCellRenderArgs<UpdateTreeEntry>| {
                        render_version_column(args.record(), args.is_expanded())
                    },
                ))
                .into(),
            DataTableColumn::new(tr!("Update Status"))
                .flex(1)
                .render_cell(DataTableCellRenderer::new(
                    move |args: &mut DataTableCellRenderArgs<UpdateTreeEntry>| {
                        render_update_status_column(args.record(), args.is_expanded())
                    },
                ))
                .into(),
            DataTableColumn::new(tr!("Repository Status"))
                .flex(1)
                .render_cell(DataTableCellRenderer::new(
                    move |args: &mut DataTableCellRenderArgs<UpdateTreeEntry>| {
                        render_repo_status_column(args.record(), args.is_expanded())
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
            product_version: None,
            mixed_versions: MixedVersions::None,
            number_of_nodes: 0,
            number_of_updatable_nodes: 0,
            number_of_failed_nodes: 0,
            repo_status: ProductRepositoryStatus::Ok,
            poll_status: remote_summary.status.clone(),
        }));
        remote_entry.set_expanded(false);

        let mut product_versions: Vec<String> = vec![];
        let mut mixed_versions = MixedVersions::None;
        let number_of_nodes = remote_summary.nodes.len();
        let mut number_of_updatable_nodes = 0;
        let mut number_of_failed_nodes = 0;
        let mut repo_status = ProductRepositoryStatus::Ok;

        // use a BTreeMap to get a stable order. Can be removed once there is proper version
        // comparison in place.
        let nodes = BTreeMap::from_iter(remote_summary.nodes.deref());

        for (node_name, node_summary) in nodes {
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

            if node_summary.repository_status > repo_status {
                repo_status = node_summary.repository_status;
            }
            let entry = NodeEntry {
                remote: remote_name.clone(),
                node: node_name.clone(),
                ty: remote_summary.remote_type,
                summary: node_summary.clone(),
                flat: false,
            };

            if let Some(version) = get_product_version(&entry) {
                product_versions.push(version);
            }

            remote_entry.append(UpdateTreeEntry::Node(entry));
        }

        let mut product_version = None;
        if product_versions.len() == 1 {
            product_version = Some(product_versions[0].clone());
        } else if product_versions.len() > 1 {
            product_versions.sort_by(|a, b| {
                proxmox_deb_version::cmp_versions(&a, &b).unwrap_or(Ordering::Less)
            });
            product_version = product_versions.last().cloned();

            let lowest_version = product_versions.first().unwrap();
            let highest_version = product_versions.last().unwrap();

            let mut lowest = lowest_version.split('.');
            let mut highest = highest_version.split('.');

            if lowest.next().unwrap_or("~") != highest.next().unwrap_or("~") {
                mixed_versions = MixedVersions::DifferentMajor;
            } else if lowest.next().unwrap_or("~") != highest.next().unwrap_or("~") {
                mixed_versions = MixedVersions::DifferentMinor;
            } else if lowest.next().unwrap_or("~") != highest.next().unwrap_or("~") {
                mixed_versions = MixedVersions::DifferentPatch;
            }
        }

        if let UpdateTreeEntry::Remote(info) = remote_entry.record_mut() {
            info.number_of_updatable_nodes = number_of_updatable_nodes;
            info.number_of_nodes = number_of_nodes as u32;
            info.number_of_failed_nodes = number_of_failed_nodes as u32;
            info.product_version = product_version;
            info.mixed_versions = mixed_versions;
            info.repo_status = repo_status;
        }
    }

    tree
}

#[derive(PartialEq)]
enum ViewState {
    ShowSubscriptionAlert,
}

impl LoadableComponent for UpdateTreeComponent {
    type Properties = UpdateTree;
    type Message = RemoteUpdateTreeMsg;
    type ViewState = ViewState;

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
            refreshing: false,
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

    fn dialog_view(
        &self,
        ctx: &LoadableComponentContext<Self>,
        view_state: &Self::ViewState,
    ) -> Option<Html> {
        let link = ctx.link().clone();
        match view_state {
            ViewState::ShowSubscriptionAlert => Some(
                pdm_subscription_alert(move |_| {
                    link.change_view(None);
                    link.send_message(RemoteUpdateTreeMsg::RefreshAll);
                })
                .into(),
            ),
        }
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Self::Message::LoadFinished(updates) => {
                let data = build_store_from_response(updates);
                self.store.write().update_root_tree(data);
                self.store.set_sorter(default_sorter);

                return true;
            }
            Self::Message::RefreshFinished => {
                self.refreshing = false;
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
            Self::Message::CheckSubscription => {
                let link = ctx.link();

                self.refreshing = true;
                link.clone().spawn(async move {
                    // Use the PDM subscription check for the global refresh all.
                    let is_active = check_pdm_subscription().await;
                    if !is_active {
                        link.change_view(Some(ViewState::ShowSubscriptionAlert));
                    } else {
                        link.send_message(RemoteUpdateTreeMsg::RefreshAll);
                    }
                    link.send_message(RemoteUpdateTreeMsg::RefreshFinished);
                });
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
            .show_header(true)
            .class(css::FlexFit);

        let refresh_all_button = Button::new(tr!("Refresh all"))
            .disabled(self.refreshing)
            .on_activate({
                let link = ctx.link().clone();
                move |_| {
                    link.send_message(RemoteUpdateTreeMsg::CheckSubscription);
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
            .with_tool(Tooltip::new(refresh_all_button).tip(tr!(
                "Refresh the status of the repository and pending updates for all remotes"
            )))
            .style("flex", "1 1 0")
            .class(FlexFit)
            .border(true)
            .with_optional_child(
                self.refreshing.then_some(
                    Container::new().style("position", "relative").with_child(
                        Progress::new()
                            .style("position", "absolute")
                            .style("left", "0")
                            .style("right", "0"),
                    ),
                ),
            )
            .with_child(table)
    }

    fn render_update_list_panel(&self, ctx: &LoadableComponentContext<Self>) -> Panel {
        let mut panel = Panel::new()
            .class(FlexFit)
            .border(true)
            .min_width(500)
            .style("flex", "1 1 0");

        match &self.selected_entry {
            Some(UpdateTreeEntry::Node(NodeEntry {
                remote,
                node,
                ty,
                summary,
                ..
            })) => {
                let title: Html = Row::new()
                    .gap(2)
                    .class(AlignItems::Baseline)
                    .with_child(Fa::new("list"))
                    // TRANSLATORS: The first parameter is the name of the remote, the second one
                    // the name of the node.
                    .with_child(tr!("Update List - {0} ({1})", remote, node))
                    .into();

                if summary.status == NodeUpdateStatus::Success {
                    let base_url = format!("/{ty}/remotes/{remote}/nodes/{node}/apt",);
                    let subscription_url =
                        format!("/{ty}/remotes/{remote}/nodes/{node}/subscription");
                    let task_base_url = format!("/{ty}/remotes/{remote}/tasks");

                    let apt = AptPackageManager::new()
                        .base_url(base_url.clone())
                        .task_base_url(task_base_url)
                        .enable_upgrade(true)
                        .subscription_url(subscription_url.clone())
                        .on_upgrade({
                            let remote = remote.clone();
                            let link = ctx.link().clone();
                            let remote = remote.clone();
                            let node = node.clone();
                            let ty = *ty;

                            move |_| match ty {
                                RemoteType::Pve => {
                                    let id = format!("node/{node}::apt");
                                    if let Some(url) =
                                        get_deep_url(link.yew_link(), &remote, None, &id)
                                    {
                                        let _ = gloo_utils::window().open_with_url(&url.href());
                                    }
                                }
                                RemoteType::Pbs => {
                                    let hash = "#pbsServerAdministration:updates";
                                    if let Some(url) =
                                        get_deep_url_low_level(link.yew_link(), &remote, None, hash)
                                    {
                                        let _ = gloo_utils::window().open_with_url(&url.href());
                                    }
                                }
                            }
                        });

                    let product = match ty {
                        RemoteType::Pve => ExistingProduct::PVE,
                        RemoteType::Pbs => ExistingProduct::PBS,
                    };

                    let repo_status = Container::new().min_height(150).with_child(
                        AptRepositories::new()
                            .product(product)
                            .status_only(true)
                            .subscription_url(subscription_url)
                            .base_url(base_url),
                    );

                    panel = panel
                        .title(title)
                        .with_child(repo_status)
                        .with_child(
                            html! {<div role="separator" class="pwt-w-100 pwt-horizontal-rule"/>},
                        )
                        .with_child(apt);
                } else {
                    let error_widget = pwt::widget::error_message(&tr!(
                        "Could not fetch update status: {0}",
                        summary.status_message.as_deref().unwrap_or_default()
                    ));

                    panel = panel.title(title).with_child(error_widget);
                }
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

                panel = panel.title(title).with_child(select_node_msg)
            }
        }

        panel
    }
}

fn render_update_status_column(tree_entry: &UpdateTreeEntry, expanded: bool) -> Html {
    match tree_entry {
        UpdateTreeEntry::Root => {
            html!()
        }
        UpdateTreeEntry::Remote(remote_info) => {
            render_remote_update_status(remote_info, expanded).into()
        }
        UpdateTreeEntry::Node(node_info) => render_node_update_status(node_info).into(),
    }
}

fn render_remote_update_status(entry: &RemoteEntry, expanded: bool) -> Row {
    let mut row = Row::new().class(css::AlignItems::Baseline).gap(2);
    match entry.poll_status {
        RemoteUpdateStatus::Success => {
            if !expanded {
                let up_to_date_nodes = entry.number_of_nodes
                    - entry.number_of_updatable_nodes
                    - entry.number_of_failed_nodes;

                if entry.number_of_nodes == up_to_date_nodes {
                    row = row.with_child(render_status_icon(StatusIcon::Ok));
                } else if entry.number_of_updatable_nodes > 0 {
                    row = row.with_child(render_status_icon(StatusIcon::Updatable));

                    if entry.number_of_failed_nodes > 0 {
                        row = row.with_child(render_status_icon(StatusIcon::Error));
                    }
                } else if entry.number_of_failed_nodes > 0 {
                    row = row.with_child(render_status_icon(StatusIcon::Error));
                }
            }
        }
        RemoteUpdateStatus::Error => {
            row = row.with_child(render_status_icon(StatusIcon::Error));
        }
        RemoteUpdateStatus::Unknown => {
            row = row.with_child(render_status_icon(StatusIcon::Unknown));
        }
    }

    row
}

fn render_node_update_status(entry: &NodeEntry) -> Html {
    let mut row = Row::new().class(css::AlignItems::Baseline).gap(2);

    let tooltip = if entry.summary.status == NodeUpdateStatus::Error {
        row = row.with_child(render_status_icon(StatusIcon::Error));
        if let Some(status) = &entry.summary.status_message {
            tr!("Failed to retrieve update status: {0}", status)
        } else {
            tr!("Unknown error")
        }
    } else if entry.summary.number_of_updates > 0 {
        row = row.with_child(render_status_icon(StatusIcon::Updatable));
        row = row.with_child(format!("{}", entry.summary.number_of_updates));
        tr!("One update pending" | "{n} updates pending" % entry.summary.number_of_updates)
    } else {
        row = row.with_child(render_status_icon(StatusIcon::Ok));
        tr!("Up-to-date")
    };

    Tooltip::new(row).tip(tooltip).into()
}

fn render_repo_status_column(tree_entry: &UpdateTreeEntry, expanded: bool) -> Html {
    match tree_entry {
        UpdateTreeEntry::Root => {
            html!()
        }
        UpdateTreeEntry::Remote(remote_info) => {
            if !expanded {
                render_repo_status(remote_info.repo_status)
            } else {
                html!()
            }
        }
        UpdateTreeEntry::Node(node_info) => {
            render_repo_status(node_info.summary.repository_status).into()
        }
    }
}

fn render_repo_status(status: ProductRepositoryStatus) -> Html {
    let mut row = Row::new().class(css::AlignItems::Baseline).gap(2);

    let tooltip = match status {
        ProductRepositoryStatus::Ok => {
            row = row.with_child(render_status_icon(StatusIcon::Ok));
            tr!("Production-ready enterprise repository enabled")
        }
        ProductRepositoryStatus::NoProductRepository => {
            row = row.with_child(render_status_icon(StatusIcon::Error));
            tr!("No product repository configured")
        }
        ProductRepositoryStatus::MissingSubscriptionForEnterprise => {
            row = row.with_child(render_status_icon(StatusIcon::Warning));
            tr!("Enterprise repository configured, but missing subscription")
        }
        ProductRepositoryStatus::NonProductionReady => {
            row = row.with_child(render_status_icon(StatusIcon::Ok));
            row = row.with_child(render_status_icon(StatusIcon::Warning));
            tr!("Non-production-ready repositories enabled")
        }
        ProductRepositoryStatus::Error => {
            row = row.with_child(render_status_icon(StatusIcon::Error));
            tr!("Error")
        }
    };

    Tooltip::new(row).tip(tooltip).into()
}

fn render_version_column(tree_entry: &UpdateTreeEntry, expanded: bool) -> Html {
    match tree_entry {
        UpdateTreeEntry::Node(node_entry) => {
            get_product_version(node_entry).unwrap_or_default().into()
        }
        UpdateTreeEntry::Remote(remote_entry) => {
            let (icon, extra) = match remote_entry.mixed_versions {
                MixedVersions::None => ("", "".to_string()),
                MixedVersions::DifferentMajor => ("times-circle", tr!("major difference")),
                MixedVersions::DifferentMinor => {
                    ("exclamation-triangle", tr!("substantial difference"))
                }
                MixedVersions::DifferentPatch => ("info-circle", tr!("small difference")),
            };
            let version_string = remote_entry.product_version.clone().unwrap_or_default();
            let text = if !expanded {
                if extra.len() > 0 {
                    format!("{version_string} {extra}")
                } else {
                    version_string
                }
            } else {
                extra
            };
            if icon.len() > 0 {
                Row::new()
                    .class(css::AlignItems::Baseline)
                    .gap(2)
                    .with_child(text)
                    .with_child(Fa::new(icon))
                    .into()
            } else {
                html! { text }
            }
        }
        _ => "".to_string().into(),
    }
    .into()
}

fn get_product_version(node_entry: &NodeEntry) -> Option<String> {
    let package = match node_entry.ty {
        RemoteType::Pve => "pve-manager",
        RemoteType::Pbs => "proxmox-backup-server",
    };

    node_entry
        .summary
        .versions
        .iter()
        .find(|p| p.package == package)
        .map(|p| p.version.to_string())
}

enum StatusIcon {
    Ok,
    Updatable,
    Error,
    Warning,
    Unknown,
}

fn render_status_icon(icon: StatusIcon) -> Fa {
    let (icon_class, icon_scheme) = match icon {
        StatusIcon::Ok => ("check", FontColor::Success),
        StatusIcon::Error => ("times-circle", FontColor::Error),
        StatusIcon::Warning => ("exclamation-triangle", FontColor::Warning),
        StatusIcon::Updatable => ("refresh", FontColor::Primary),
        StatusIcon::Unknown => ("question-circle-o", FontColor::Primary),
    };

    Fa::new(icon_class).class(icon_scheme)
}
