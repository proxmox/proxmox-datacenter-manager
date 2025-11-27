use std::rc::Rc;

use yew::{
    html,
    virtual_dom::{Key, VComp, VNode},
    Component, Html, Properties,
};

use pdm_api_types::subscription::{
    RemoteSubscriptionState, RemoteSubscriptions, SubscriptionLevel,
};
use proxmox_yew_comp::Status;
use pwt::{
    css::{AlignItems, Overflow},
    props::{ContainerBuilder, ExtractPrimaryKey, WidgetBuilder},
    state::{KeyedSlabTree, TreeStore},
    tr,
    widget::{
        data_table::{DataTable, DataTableColumn, DataTableHeader},
        Container, Fa, Row,
    },
};

#[derive(Properties, PartialEq)]
pub struct SubscriptionsList {
    subscriptions: Vec<RemoteSubscriptions>,
}

impl SubscriptionsList {
    pub fn new(subscriptions: Vec<RemoteSubscriptions>) -> Self {
        yew::props!(Self { subscriptions })
    }
}

pub struct PdmSubscriptionsList {
    store: TreeStore<SubscriptionTreeEntry>,
}

#[derive(Clone, PartialEq)]
struct RemoteEntry {
    name: String,
    state: RemoteSubscriptionState,
    error: Option<String>,
}

#[derive(Clone, PartialEq)]
struct NodeEntry {
    remote: String,
    name: String,
    level: SubscriptionLevel,
    standalone: bool,
}

#[derive(Clone, PartialEq)]
enum SubscriptionTreeEntry {
    Root,
    Remote(RemoteEntry),
    Node(NodeEntry),
}

impl SubscriptionTreeEntry {
    fn name(&self) -> &str {
        match self {
            SubscriptionTreeEntry::Root => "",
            SubscriptionTreeEntry::Remote(remote_entry) => &remote_entry.name,
            SubscriptionTreeEntry::Node(node_entry) => &node_entry.name,
        }
    }
}

impl ExtractPrimaryKey for SubscriptionTreeEntry {
    fn extract_key(&self) -> Key {
        match self {
            SubscriptionTreeEntry::Root => Key::from("root"),
            SubscriptionTreeEntry::Remote(remote) => Key::from(format!("{}", remote.name)),
            SubscriptionTreeEntry::Node(node) => {
                Key::from(format!("{}/{}", node.remote, node.name))
            }
        }
    }
}

impl PdmSubscriptionsList {
    fn update_store_data(&self, ctx: &yew::Context<Self>) {
        let mut store = self.store.write();
        let mut tree = KeyedSlabTree::new();
        let mut root = tree.set_root(SubscriptionTreeEntry::Root);
        root.set_expanded(true);
        let subscriptions = sort_subscriptions(&ctx.props().subscriptions);

        for remote in subscriptions {
            match remote.node_status {
                Some(node_status) if node_status.len() == 1 => {
                    let (node_name, info) = node_status.into_iter().next().unwrap();
                    if let Some(info) = info {
                        root.append(SubscriptionTreeEntry::Node(NodeEntry {
                            remote: remote.remote.clone(),
                            name: node_name.clone(),
                            level: info.level,
                            standalone: true,
                        }));
                    }
                    continue;
                }
                _ => {
                    let mut remote_node = root.append(SubscriptionTreeEntry::Remote(RemoteEntry {
                        name: remote.remote.clone(),
                        state: remote.state.clone(),
                        error: remote.error.clone(),
                    }));

                    if let Some(node_status) = remote.node_status.as_ref() {
                        if node_status.is_empty() {
                            continue;
                        }

                        for (node_name, info) in node_status {
                            if let Some(info) = info {
                                remote_node.append(SubscriptionTreeEntry::Node(NodeEntry {
                                    remote: remote.remote.clone(),
                                    name: node_name.clone(),
                                    level: info.level,
                                    standalone: false,
                                }));
                            }
                        }
                    }
                }
            }
        }
        store.update_root_tree(tree);
    }
}

impl Component for PdmSubscriptionsList {
    type Message = ();
    type Properties = SubscriptionsList;

    fn create(ctx: &yew::Context<Self>) -> Self {
        let this = Self {
            store: TreeStore::new().view_root(false),
        };
        this.update_store_data(ctx);
        this
    }

    fn changed(&mut self, ctx: &yew::Context<Self>, _old_props: &Self::Properties) -> bool {
        self.update_store_data(ctx);
        true
    }

    fn view(&self, _ctx: &yew::Context<Self>) -> Html {
        DataTable::new(columns(self.store.clone()), self.store.clone())
            .class(Overflow::Auto)
            .into()
    }
}

fn columns(
    store: TreeStore<SubscriptionTreeEntry>,
) -> Rc<Vec<DataTableHeader<SubscriptionTreeEntry>>> {
    let tree_column = DataTableColumn::new(tr!("Remote"))
        .tree_column(store)
        .render(|entry: &SubscriptionTreeEntry| {
            let row = Row::new().class(AlignItems::Center).gap(2);
            match entry {
                SubscriptionTreeEntry::Remote(remote) => row
                    .with_child(Fa::new("server").fixed_width())
                    .with_child(remote.name.clone())
                    .into(),
                SubscriptionTreeEntry::Node(node) => row
                    .with_child(Fa::new("building").fixed_width())
                    .with_child(if node.standalone {
                        format!("{} - {}", node.remote, node.name)
                    } else {
                        node.name.clone()
                    })
                    .into(),
                SubscriptionTreeEntry::Root => row.into(),
            }
        })
        .sorter(|a: &SubscriptionTreeEntry, b: &SubscriptionTreeEntry| a.name().cmp(b.name()))
        .into();

    fn render_subscription_state(state: &RemoteSubscriptionState) -> Row {
        let icon = match state {
            RemoteSubscriptionState::Mixed => Fa::from(Status::Warning),
            RemoteSubscriptionState::Active => Fa::from(Status::Success),
            RemoteSubscriptionState::None => Fa::from(Status::Error),
            _ => Fa::from(Status::Unknown),
        };

        let text = match state {
            RemoteSubscriptionState::None => "None",
            RemoteSubscriptionState::Unknown => "Unknown",
            RemoteSubscriptionState::Mixed => "Mixed",
            RemoteSubscriptionState::Active => "Active",
        };

        Row::new()
            .class(AlignItems::Center)
            .gap(2)
            .with_child(icon)
            .with_child(Container::from_tag("span").with_child(text))
    }

    fn render_subscription_level(level: SubscriptionLevel) -> &'static str {
        match level {
            SubscriptionLevel::None => "None",
            SubscriptionLevel::Basic => "Basic",
            SubscriptionLevel::Community => "Community",
            SubscriptionLevel::Premium => "Premium",
            SubscriptionLevel::Standard => "Standard",
            SubscriptionLevel::Unknown => "Unknown",
        }
    }

    let subscription_column = DataTableColumn::new(tr!("Subscription"))
        .render(|entry: &SubscriptionTreeEntry| match entry {
            SubscriptionTreeEntry::Node(node) => {
                if node.standalone {
                    let (sub_state, text) = match node.level {
                        SubscriptionLevel::None => (RemoteSubscriptionState::None, None),
                        SubscriptionLevel::Unknown => (RemoteSubscriptionState::Unknown, None),
                        other => (
                            RemoteSubscriptionState::Active,
                            Some(render_subscription_level(other)),
                        ),
                    };
                    render_subscription_state(&sub_state)
                        .with_optional_child(text)
                        .into()
                } else {
                    render_subscription_level(node.level).into()
                }
            }
            SubscriptionTreeEntry::Remote(remote) => {
                if let Some(error) = &remote.error {
                    html! { <span class="pwt-font-label-small">{error}</span> }
                } else {
                    render_subscription_state(&remote.state).into()
                }
            }
            SubscriptionTreeEntry::Root => "".into(),
        })
        .into();

    Rc::new(vec![tree_column, subscription_column])
}

fn sort_subscriptions(subs: &[RemoteSubscriptions]) -> Vec<RemoteSubscriptions> {
    let mut subscriptions = subs.to_vec();
    subscriptions.sort_by_key(|rs| match rs.state {
        RemoteSubscriptionState::None => 0,
        RemoteSubscriptionState::Unknown => 1,
        RemoteSubscriptionState::Mixed => 2,
        RemoteSubscriptionState::Active => 3,
    });
    subscriptions
}

impl From<SubscriptionsList> for VNode {
    fn from(val: SubscriptionsList) -> Self {
        let comp = VComp::new::<PdmSubscriptionsList>(Rc::new(val), None);
        VNode::from(comp)
    }
}
