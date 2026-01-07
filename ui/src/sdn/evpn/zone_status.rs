use std::cmp::Ordering;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::{Context, Error};
use yew::virtual_dom::{Key, VComp, VNode};
use yew::{html, AttrValue, Properties};

use pdm_client::types::SdnZoneIpVrf;
use proxmox_yew_comp::{
    LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState,
};
use pwt::props::{ContainerBuilder, FieldBuilder, WidgetBuilder, WidgetStyleBuilder};
use pwt::props::{EventSubscriber, ExtractPrimaryKey};
use pwt::state::Store;
use pwt::tr;
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::Combobox;
use pwt::widget::{error_message, Button, Column, InputPanel, Toolbar};

use crate::pdm_client;
use crate::sdn::evpn::NodeList;

#[derive(Clone, PartialEq, Properties, Default)]
pub struct ZoneStatusTable {
    remote: String,
    zone: String,
    nodes: Option<NodeList>,
}

impl ZoneStatusTable {
    pub fn new(remote: String, zone: String, nodes: Option<NodeList>) -> Self {
        yew::props!(Self {
            zone,
            remote,
            nodes
        })
    }
}

impl From<ZoneStatusTable> for VNode {
    fn from(value: ZoneStatusTable) -> Self {
        let comp = VComp::new::<LoadableComponentMaster<ZoneStatusComponent>>(Rc::new(value), None);
        VNode::from(comp)
    }
}

#[derive(Clone, PartialEq)]
#[repr(transparent)]
pub struct IpVrfEntry(pub SdnZoneIpVrf);

impl From<SdnZoneIpVrf> for IpVrfEntry {
    fn from(value: SdnZoneIpVrf) -> Self {
        Self(value)
    }
}

impl ExtractPrimaryKey for IpVrfEntry {
    fn extract_key(&self) -> Key {
        Key::from(format!("{}/{}", self.0.ip, self.0.metric))
    }
}

fn default_sorter(a: &IpVrfEntry, b: &IpVrfEntry) -> Ordering {
    (&a.0.ip, &a.0.metric).cmp(&(&b.0.ip, &b.0.metric))
}

struct ZoneStatusComponent {
    state: LoadableComponentState<()>,
    store: Store<IpVrfEntry>,
    columns: Rc<Vec<DataTableHeader<IpVrfEntry>>>,
    nodes: Option<Rc<Vec<AttrValue>>>,
    selected_node: Option<AttrValue>,
    error_msg: Option<String>,
    vrf_loading: bool,
}

pwt::impl_deref_mut_property!(ZoneStatusComponent, state, LoadableComponentState<()>);

impl ZoneStatusComponent {
    fn columns() -> Rc<Vec<DataTableHeader<IpVrfEntry>>> {
        Rc::new(vec![
            DataTableColumn::new(tr!("Destination"))
                .get_property(|entry: &IpVrfEntry| &entry.0.ip)
                .into(),
            DataTableColumn::new(tr!("Nexthops"))
                .render(|entry: &IpVrfEntry| {
                    let mut column = Column::new();

                    for nexthop in &entry.0.nexthops {
                        column.add_child(html! { <div>{ nexthop }</div> });
                    }

                    column.into()
                })
                .into(),
            DataTableColumn::new(tr!("Protocol"))
                .get_property(|entry: &IpVrfEntry| &entry.0.protocol)
                .into(),
            DataTableColumn::new(tr!("Metric"))
                .get_property(|entry: &IpVrfEntry| &entry.0.metric)
                .into(),
        ])
    }
}

#[derive(Debug)]
enum ZoneStatusComponentMsg {
    NodeSelected(Option<String>),
    NodeListLoaded(Rc<Vec<AttrValue>>),
    ZoneStatusLoaded(Result<Vec<SdnZoneIpVrf>, Error>),
}

impl LoadableComponent for ZoneStatusComponent {
    type Message = ZoneStatusComponentMsg;
    type Properties = ZoneStatusTable;
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

                    self.spawn(async move {
                        let status_result = pdm_client()
                            .pve_sdn_zone_get_ip_vrf(&props.remote, &node_name, &props.zone)
                            .await;

                        link.send_message(Self::Message::ZoneStatusLoaded(
                            status_result.with_context(|| "could not load zone status".to_string()),
                        ));
                    });
                }
            }
            Self::Message::ZoneStatusLoaded(zone_status_result) => {
                self.vrf_loading = false;

                match zone_status_result {
                    Ok(zone_status) => {
                        self.store
                            .write()
                            .set_data(zone_status.into_iter().map(IpVrfEntry::from).collect());

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
                        .key("zone-node-combobox")
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
