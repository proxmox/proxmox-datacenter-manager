use std::rc::Rc;

use anyhow::Error;
use yew::virtual_dom::{VComp, VNode};

use pdm_search::{Search, SearchTerm};
use proxmox_yew_comp::Status;
use pwt::css::{AlignItems, FlexFit, JustifyContent, TextAlign};
use pwt::prelude::*;
use pwt::state::SharedState;
use pwt::widget::{error_message, Column, Fa, Panel};

use pdm_api_types::resource::NodeStatusCount;
use pdm_api_types::{remotes::RemoteType, resource::ResourcesStatus};

use crate::dashboard::create_title_with_icon;
use crate::dashboard::view::add_current_view_to_search;
use crate::search_provider::get_search_provider;
use crate::LoadResult;

use super::loading_column;

#[derive(PartialEq, Clone, Properties)]
pub struct NodeStatusPanel {
    remote_type: Option<RemoteType>,
    status: Option<NodeStatusCount>,
    failed_remotes: usize,
}

impl NodeStatusPanel {
    /// Create a node status panel.
    ///
    /// Passing `None` to `remote_type` means creating a panel for all nodes, regardless of remote type.
    pub fn new(
        remote_type: Option<RemoteType>,
        status: Option<NodeStatusCount>,
        failed_remotes: usize,
    ) -> Self {
        yew::props!(Self {
            remote_type,
            status,
            failed_remotes,
        })
    }
}

impl From<NodeStatusPanel> for VNode {
    fn from(value: NodeStatusPanel) -> Self {
        let comp = VComp::new::<NodeStatusPanelComponent>(Rc::new(value), None);
        VNode::from(comp)
    }
}

pub struct NodeStatusPanelComponent {}

impl yew::Component for NodeStatusPanelComponent {
    type Message = Search;
    type Properties = NodeStatusPanel;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self {}
    }

    fn update(&mut self, ctx: &Context<Self>, mut msg: Self::Message) -> bool {
        if let Some(provider) = get_search_provider(ctx) {
            add_current_view_to_search(ctx, &mut msg);
            provider.search(msg);
        }
        false
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();

        let (icon, status_msg, search_terms) = match &props.status {
            Some(status) => map_status(status, props.remote_type, props.failed_remotes),
            None => return loading_column().into(),
        };

        let column = Column::new()
            .padding(4)
            .class("pwt-pointer")
            .class(FlexFit)
            .class(AlignItems::Center)
            .class(JustifyContent::Center)
            .class(TextAlign::Center)
            .gap(2)
            .onclick(ctx.link().callback({
                let search_terms = search_terms.clone();
                move |_| Search::with_terms(search_terms.clone())
            }))
            .onkeydown(ctx.link().batch_callback({
                let search_terms = search_terms.clone();
                move |event: KeyboardEvent| match event.key().as_str() {
                    "Enter" | " " => Some(Search::with_terms(search_terms.clone())),
                    _ => None,
                }
            }))
            .with_child(icon.large_4x())
            .with_child(status_msg);
        column.into()
    }
}

fn map_status(
    status: &NodeStatusCount,
    remote_type: Option<RemoteType>,
    failed_remotes: usize,
) -> (Fa, String, Vec<SearchTerm>) {
    let mut search_terms = vec![SearchTerm::new("node").category(Some("type"))];

    if let Some(remote_type) = remote_type {
        search_terms.push(SearchTerm::new(remote_type.to_string()).category(Some("remote-type")));
    }
    let (icon, status_msg) = match status {
        NodeStatusCount {
            online,
            offline,
            unknown,
        } if *offline > 0 => {
            search_terms.push(SearchTerm::new("offline").category(Some("status")));
            (
                Status::Error.into(),
                tr!(
                    "{0} of {1} nodes are offline",
                    offline,
                    online + offline + unknown,
                ),
            )
        }
        NodeStatusCount { unknown, .. } if *unknown > 0 => {
            search_terms.push(SearchTerm::new("unknown").category(Some("status")));
            (
                Status::Warning.into(),
                tr!("{0} nodes have an unknown status", unknown),
            )
        }
        NodeStatusCount { online, .. } if failed_remotes > 0 => match remote_type {
            Some(RemoteType::Pve) | None => (
                Status::Unknown.into(),
                tr!("{0} of an unknown number of nodes online", online),
            ),
            Some(RemoteType::Pbs) => (
                Status::Error.into(),
                tr!("{0} remotes failed", failed_remotes),
            ),
        },
        NodeStatusCount { online, .. } => (Status::Success.into(), tr!("{0} nodes online", online)),
    };

    (icon, status_msg, search_terms)
}

/// Create a node status panel.
///
/// Passing `None` to `remote_type` means creating a panel for all nodes, regardless of remote type.
pub fn create_node_panel(
    remote_type: Option<RemoteType>,
    status: SharedState<LoadResult<ResourcesStatus, Error>>,
) -> Panel {
    let (icon, title) = match remote_type {
        Some(RemoteType::Pve) => ("building", tr!("Virtual Environment Nodes")),
        Some(RemoteType::Pbs) => ("building-o", tr!("Backup Server Nodes")),
        None => ("building", tr!("Nodes")),
    };

    let status = status.read();

    let (nodes_status, failed_remotes) = match &status.data {
        Some(status) => {
            let nodes_status = match remote_type {
                Some(RemoteType::Pve) => Some(status.pve_nodes.clone()),
                Some(RemoteType::Pbs) => Some(status.pbs_nodes.clone()),
                None => Some(NodeStatusCount {
                    online: status.pve_nodes.online + status.pbs_nodes.online,
                    offline: status.pve_nodes.offline + status.pbs_nodes.offline,
                    unknown: status.pve_nodes.unknown + status.pbs_nodes.unknown,
                }),
            };
            let failed_remotes = status
                .failed_remotes_list
                .iter()
                .filter(|item| match remote_type {
                    Some(remote_type) => item.remote_type == remote_type,
                    None => true,
                })
                .count();
            (nodes_status, failed_remotes)
        }
        None => (None, 0),
    };

    Panel::new()
        .title(create_title_with_icon(icon, title))
        .border(true)
        .with_child(NodeStatusPanel::new(
            remote_type,
            nodes_status,
            failed_remotes,
        ))
        .with_optional_child(
            status
                .error
                .as_ref()
                .map(|err| error_message(&err.to_string())),
        )
}
