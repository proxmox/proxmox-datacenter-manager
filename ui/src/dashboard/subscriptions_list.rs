use std::rc::Rc;

use yew::{
    html,
    virtual_dom::{Key, VComp, VNode},
    Html,
    Component, Properties
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
        Fa, Row,
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

impl Component for PdmSubscriptionsList {
    type Message = ();
    type Properties = SubscriptionsList;

    fn create(ctx: &yew::Context<Self>) -> Self {
        let subscriptions = sort_subscriptions(&ctx.props().subscriptions);

        let store = TreeStore::new().view_root(false);
        let mut tree = KeyedSlabTree::new();
        let mut root = tree.set_root(SubscriptionTreeEntry::Root);
        root.set_expanded(true);

        for remote in subscriptions {
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
                            level: info.level.clone(),
                        }));
                    }
                }
            }
        }

        store.write().update_root_tree(tree);
        Self { store }
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
                SubscriptionTreeEntry::Remote(remote) => row.with_child(remote.name.clone()).into(),
                SubscriptionTreeEntry::Node(node) => row
                    .with_child(Fa::new("server"))
                    .with_child(node.name.clone())
                    .into(),
                SubscriptionTreeEntry::Root => row.into(),
            }
        })
        .sorter(|a: &SubscriptionTreeEntry, b: &SubscriptionTreeEntry| a.name().cmp(b.name()))
        .into();

    let subscription_column = DataTableColumn::new(tr!("Subscription"))
        .render(|entry: &SubscriptionTreeEntry| match entry {
            SubscriptionTreeEntry::Node(node) => {
                let text = match node.level {
                    SubscriptionLevel::None => "None",
                    SubscriptionLevel::Basic => "Basic",
                    SubscriptionLevel::Community => "Community",
                    SubscriptionLevel::Premium => "Premium",
                    SubscriptionLevel::Standard => "Standard",
                    SubscriptionLevel::Unknown => "Unknown",
                };
                text.into()
            }
            SubscriptionTreeEntry::Remote(remote) => {
                if let Some(error) = &remote.error {
                    html! { <span class="pwt-font-label-small">{error}</span> }.into()
                } else {
                    let icon = match remote.state {
                        RemoteSubscriptionState::Mixed => Fa::from(Status::Warning),
                        RemoteSubscriptionState::Active => Fa::from(Status::Success),
                        _ => Fa::from(Status::Unknown),
                    };

                    let text = match remote.state {
                        RemoteSubscriptionState::None => "None",
                        RemoteSubscriptionState::Unknown => "Unknown",
                        RemoteSubscriptionState::Mixed => "Mixed",
                        RemoteSubscriptionState::Active => "Active",
                    };

                    Row::new().gap(2).with_child(icon).with_child(text).into()
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
