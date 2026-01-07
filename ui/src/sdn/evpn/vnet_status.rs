use std::cmp::Ordering;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::{Context, Error};

use proxmox_yew_comp::{
    LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState,
};
use pwt::props::ExtractPrimaryKey;
use yew::virtual_dom::{Key, VComp, VNode};
use yew::{AttrValue, Properties};

use pdm_client::types::SdnVnetMacVrf;
use pwt::props::{
    ContainerBuilder, EventSubscriber, FieldBuilder, WidgetBuilder, WidgetStyleBuilder,
};
use pwt::state::Store;
use pwt::tr;
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::Combobox;
use pwt::widget::{error_message, Button, Column, InputPanel, Toolbar};

use crate::pdm_client;
use crate::sdn::evpn::NodeList;

#[derive(Clone, PartialEq, Properties, Default)]
pub struct VnetStatusTable {
    remote: String,
    vnet: String,
    nodes: Option<NodeList>,
}

impl VnetStatusTable {
    pub fn new(remote: String, vnet: String, nodes: Option<NodeList>) -> Self {
        yew::props!(Self {
            vnet,
            remote,
            nodes
        })
    }
}

impl From<VnetStatusTable> for VNode {
    fn from(value: VnetStatusTable) -> Self {
        let comp = VComp::new::<LoadableComponentMaster<VnetStatusComponent>>(Rc::new(value), None);
        VNode::from(comp)
    }
}

#[derive(Clone, PartialEq)]
#[repr(transparent)]
pub struct MacVrfEntry(pub SdnVnetMacVrf);

impl From<SdnVnetMacVrf> for MacVrfEntry {
    fn from(value: SdnVnetMacVrf) -> Self {
        Self(value)
    }
}

impl ExtractPrimaryKey for MacVrfEntry {
    fn extract_key(&self) -> Key {
        Key::from(self.0.ip.as_str())
    }
}

fn default_sorter(a: &MacVrfEntry, b: &MacVrfEntry) -> Ordering {
    a.0.ip.cmp(&b.0.ip)
}

struct VnetStatusComponent {
    state: LoadableComponentState<()>,
    store: Store<MacVrfEntry>,
    columns: Rc<Vec<DataTableHeader<MacVrfEntry>>>,
    nodes: Option<Rc<Vec<AttrValue>>>,
    selected_node: Option<AttrValue>,
    error_msg: Option<String>,
    vrf_loading: bool,
}

pwt::impl_deref_mut_property!(VnetStatusComponent, state, LoadableComponentState<()>);

impl VnetStatusComponent {
    fn columns() -> Rc<Vec<DataTableHeader<MacVrfEntry>>> {
        Rc::new(vec![
            DataTableColumn::new(tr!("IP Address"))
                .get_property(|entry: &MacVrfEntry| &entry.0.ip)
                .into(),
            DataTableColumn::new(tr!("MAC Address"))
                .get_property(|entry: &MacVrfEntry| &entry.0.mac)
                .into(),
            DataTableColumn::new(tr!("via"))
                .get_property(|entry: &MacVrfEntry| &entry.0.nexthop)
                .into(),
        ])
    }
}

#[derive(Debug)]
enum VnetStatusComponentMsg {
    NodeSelected(Option<String>),
    NodeListLoaded(Rc<Vec<AttrValue>>),
    VnetStatusLoaded(Result<Vec<SdnVnetMacVrf>, Error>),
}

impl LoadableComponent for VnetStatusComponent {
    type Message = VnetStatusComponentMsg;
    type Properties = VnetStatusTable;
    type ViewState = ();

    fn create(_ctx: &LoadableComponentContext<Self>) -> Self {
        Self {
            state: LoadableComponentState::new(),
            store: Store::new(),
            columns: Self::columns(),
            selected_node: None,
            nodes: Default::default(),
            error_msg: None,
            vrf_loading: false,
        }
    }

    fn load(
        &self,
        ctx: &proxmox_yew_comp::LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>>>> {
        let link = ctx.link().clone();
        let props = ctx.props().clone();

        Box::pin(async move {
            let node_list = if let Some(nodes) = props.nodes {
                nodes.iter().cloned().map(AttrValue::from).collect()
            } else {
                pdm_client()
                    .pve_list_nodes(&props.remote)
                    .await?
                    .into_iter()
                    .map(|node_index| AttrValue::from(node_index.node))
                    .collect()
            };

            link.send_message(Self::Message::NodeListLoaded(Rc::new(node_list)));

            Ok(())
        })
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Self::Message::NodeListLoaded(node_list) => {
                let selected_node = node_list.iter().next().cloned();

                self.nodes = Some(node_list);

                if let Some(node) = selected_node {
                    ctx.link()
                        .send_message(Self::Message::NodeSelected(Some(node.to_string())));
                }
            }
            Self::Message::NodeSelected(node_name) => {
                if let Some(node_name) = node_name {
                    self.vrf_loading = true;
                    self.selected_node = Some(node_name.clone().into());

                    let link = ctx.link().clone();
                    let props = ctx.props().clone();

                    ctx.link().spawn(async move {
                        let status_result = pdm_client()
                            .pve_sdn_vnet_get_mac_vrf(&props.remote, &node_name, &props.vnet)
                            .await;

                        link.send_message(Self::Message::VnetStatusLoaded(
                            status_result.with_context(|| "could not load vnet status".to_string()),
                        ));
                    });
                }
            }
            Self::Message::VnetStatusLoaded(vnet_status_result) => {
                self.vrf_loading = false;

                match vnet_status_result {
                    Ok(vnet_status) => {
                        self.store
                            .write()
                            .set_data(vnet_status.into_iter().map(MacVrfEntry::from).collect());

                        self.store.set_sorter(default_sorter);

                        self.error_msg = None;
                    }
                    Err(error) => {
                        self.store.write().clear();
                        self.error_msg = Some(format!("{error:?}"));
                    }
                }
            }
        }

        true
    }

    fn main_view(&self, ctx: &proxmox_yew_comp::LoadableComponentContext<Self>) -> yew::Html {
        let selected_node = self.selected_node.clone();

        let toolbar = Toolbar::new()
            .class("pwt-w-100")
            .class("pwt-overflow-hidden")
            .class("pwt-border-bottom")
            .with_child(
                InputPanel::new().with_field(
                    tr!("Node"),
                    Combobox::new()
                        .key("vnet-node-combobox")
                        .min_width(100)
                        .required(true)
                        .value(self.selected_node.clone())
                        .items(self.nodes.clone().unwrap_or_default())
                        .on_change(
                            ctx.link()
                                .callback(|node| Self::Message::NodeSelected(Some(node))),
                        ),
                ),
            )
            .with_flex_spacer()
            .with_child(Button::refresh(self.loading() || self.vrf_loading).onclick(
                ctx.link().callback(move |_| {
                    Self::Message::NodeSelected(selected_node.as_ref().map(ToString::to_string))
                }),
            ));

        let table =
            DataTable::new(self.columns.clone(), self.store.clone()).class(pwt::css::FlexFit);

        let mut column = Column::new()
            .class(pwt::css::FlexFit)
            .with_child(toolbar)
            .with_child(table);

        if let Some(msg) = &self.error_msg {
            column.add_child(error_message(msg));
        }

        column.into()
    }

    fn changed(
        &mut self,
        ctx: &LoadableComponentContext<Self>,
        _old_props: &Self::Properties,
    ) -> bool {
        self.selected_node = None;
        self.nodes = None;

        ctx.link().send_reload();

        true
    }
}
