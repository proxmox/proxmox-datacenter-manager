use futures::try_join;
use std::rc::Rc;
use std::str::FromStr;

use anyhow::Error;
use yew::virtual_dom::{VComp, VNode};
use yew::{html, Callback, Html, Properties};

use pdm_client::types::{ListController, ListControllersType, ListVnet, ListZone, ListZonesType};
use proxmox_yew_comp::{
    LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState,
};

use pwt::css::{AlignItems, FlexFit, JustifyContent};
use pwt::props::{
    ContainerBuilder, EventSubscriber, StorageLocation, WidgetBuilder, WidgetStyleBuilder,
};
use pwt::state::{NavigationContainer, Selection};
use pwt::tr;
use pwt::widget::menu::{Menu, MenuButton, MenuItem};
use pwt::widget::{
    Button, Column, Container, MiniScrollMode, Panel, Row, TabBarItem, TabPanel, Toolbar,
};

use crate::pdm_client;
use crate::sdn::evpn::{
    AddVnetWindow, AddZoneWindow, NodeList, RemoteTree, VnetStatusTable, VrfTree, ZoneStatusTable,
};

#[derive(PartialEq, Properties)]
pub struct EvpnPanel {}

impl Default for EvpnPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl EvpnPanel {
    pub fn new() -> Self {
        Self {}
    }
}

impl From<EvpnPanel> for VNode {
    fn from(value: EvpnPanel) -> Self {
        let comp = VComp::new::<LoadableComponentMaster<EvpnPanelComponent>>(Rc::new(value), None);
        VNode::from(comp)
    }
}

pub enum DetailPanel {
    Zone { remote: String, zone: String },
    Vnet { remote: String, vnet: String },
}

pub enum EvpnPanelMsg {
    Reload,
    LoadFinished {
        controllers: Rc<Vec<ListController>>,
        zones: Rc<Vec<ListZone>>,
        vnets: Rc<Vec<ListVnet>>,
    },
    DetailSelection(Option<DetailPanel>),
}

#[derive(Debug, PartialEq)]
pub enum EvpnPanelViewState {
    AddZone,
    AddVnet,
}

async fn load_zones() -> Result<Vec<ListZone>, Error> {
    let client = pdm_client();
    let data = client
        .pve_sdn_list_zones(false, true, ListZonesType::Evpn)
        .await?;
    Ok(data)
}

async fn load_controllers() -> Result<Vec<ListController>, Error> {
    let client = pdm_client();
    let data = client
        .pve_sdn_list_controllers(false, true, ListControllersType::Evpn)
        .await?;
    Ok(data)
}

async fn load_vnets() -> Result<Vec<ListVnet>, Error> {
    let client = pdm_client();
    let data = client.pve_sdn_list_vnets(false, true).await?;
    Ok(data)
}

pub struct EvpnPanelComponent {
    state: LoadableComponentState<EvpnPanelViewState>,
    controllers: Rc<Vec<ListController>>,
    zones: Rc<Vec<ListZone>>,
    vnets: Rc<Vec<ListVnet>>,
    initial_load: bool,
    selected_detail: Option<DetailPanel>,
    selected_tab: Selection,
}

pwt::impl_deref_mut_property!(
    EvpnPanelComponent,
    state,
    LoadableComponentState<EvpnPanelViewState>
);

impl EvpnPanelComponent {
    fn create_toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Toolbar {
        let on_add_zone = ctx
            .link()
            .change_view_callback(|_| Some(EvpnPanelViewState::AddZone));

        let on_add_vnet = ctx
            .link()
            .change_view_callback(|_| Some(EvpnPanelViewState::AddVnet));

        let on_refresh = ctx.link().callback(|_| EvpnPanelMsg::Reload);

        let add_menu = Menu::new()
            .with_item(
                MenuItem::new(tr!("Zone"))
                    .icon_class("fa fa-th")
                    .on_select(on_add_zone),
            )
            .with_item(
                MenuItem::new(tr!("VNet"))
                    .icon_class("fa fa-sdn-vnet")
                    .on_select(on_add_vnet),
            );

        Toolbar::new()
            .class("pwt-w-100")
            .class("pwt-overflow-hidden")
            .class("pwt-border-bottom")
            .with_child(MenuButton::new(tr!("Add")).show_arrow(true).menu(add_menu))
            .with_flex_spacer()
            .with_child(Button::refresh(self.loading()).onclick(on_refresh))
    }
}

impl LoadableComponent for EvpnPanelComponent {
    type Properties = EvpnPanel;
    type Message = EvpnPanelMsg;
    type ViewState = EvpnPanelViewState;

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        let link = ctx.link().clone();

        let selected_tab = Selection::new()
            .on_select(move |_| link.send_message(Self::Message::DetailSelection(None)));

        Self {
            state: LoadableComponentState::new(),
            initial_load: true,
            controllers: Default::default(),
            zones: Default::default(),
            vnets: Default::default(),
            selected_detail: None,
            selected_tab,
        }
    }

    fn load(
        &self,
        ctx: &LoadableComponentContext<Self>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), Error>>>> {
        let link = ctx.link().clone();

        Box::pin(async move {
            let (controllers, zones, vnets) =
                try_join!(load_controllers(), load_zones(), load_vnets())?;

            link.send_message(Self::Message::LoadFinished {
                controllers: Rc::new(controllers),
                zones: Rc::new(zones),
                vnets: Rc::new(vnets),
            });

            Ok(())
        })
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Self::Message::LoadFinished {
                controllers,
                zones,
                vnets,
            } => {
                self.controllers = controllers;
                self.zones = zones;
                self.vnets = vnets;
                self.initial_load = false;

                return true;
            }
            Self::Message::DetailSelection(data) => {
                self.selected_detail = data;
                return true;
            }
            Self::Message::Reload => {
                ctx.link().send_reload();
            }
        }

        false
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        let tab_panel = TabPanel::new()
            .state_id(StorageLocation::session("EvpnPanelState"))
            .class(pwt::css::FlexFit)
            .router(true)
            .scroll_mode(MiniScrollMode::Arrow)
            .selection(self.selected_tab.clone())
            .with_item(
                TabBarItem::new()
                    .key("remotes")
                    .label(tr!("Remotes"))
                    .icon_class("fa fa-server"),
                Column::new()
                    .class(pwt::css::FlexFit)
                    .with_child(self.create_toolbar(ctx))
                    .with_child(if self.initial_load {
                        VNode::from(
                            Column::new()
                                .class(FlexFit)
                                .class(AlignItems::Center)
                                .class(JustifyContent::Center)
                                .with_child(html! {<i class={"pwt-loading-icon"} />}),
                        )
                    } else {
                        VNode::from(RemoteTree::new(
                            self.zones.clone(),
                            self.vnets.clone(),
                            self.controllers.clone(),
                            ctx.link().callback(|panel: Option<DetailPanel>| {
                                Self::Message::DetailSelection(panel)
                            }),
                        ))
                    }),
            )
            .with_item(
                TabBarItem::new()
                    .key("vrfs")
                    .label(tr!("IP-VRFs"))
                    .icon_class("fa fa-th"),
                Column::new()
                    .class(pwt::css::FlexFit)
                    .with_child(self.create_toolbar(ctx))
                    .with_child(if self.initial_load {
                        VNode::from(
                            Column::new()
                                .class(FlexFit)
                                .class(AlignItems::Center)
                                .class(JustifyContent::Center)
                                .with_child(html! {<i class={"pwt-loading-icon"} />}),
                        )
                    } else {
                        VNode::from(VrfTree::new(
                            self.zones.clone(),
                            self.vnets.clone(),
                            self.controllers.clone(),
                            ctx.link().callback(|panel: Option<DetailPanel>| {
                                Self::Message::DetailSelection(panel)
                            }),
                        ))
                    }),
            );

        let navigation_container = NavigationContainer::new().with_child(tab_panel);

        let mut container = Container::new()
            .class("pwt-content-spacer")
            .class(FlexFit)
            .class("pwt-flex-direction-row")
            .with_child(Panel::new().flex(1.0).with_child(navigation_container));

        let (title, detail_html) = if let Some(detail) = &self.selected_detail {
            match detail {
                DetailPanel::Vnet {
                    remote,
                    vnet: vnet_id,
                } => {
                    let vnet = self.vnets.iter().find(|list_vnet| {
                        list_vnet.vnet.vnet.as_str() == vnet_id.as_str()
                            && list_vnet.remote.as_str() == remote
                    });

                    if let Some(vnet) = vnet {
                        let zone = self.zones.iter().find(|list_zone| {
                            list_zone.zone.zone.as_str()
                                == vnet.vnet.zone.as_deref().unwrap_or_default()
                                && list_zone.remote.as_str() == remote.as_str()
                        });

                        let node_list = zone.as_ref().and_then(|zone| {
                            let nodes = zone.zone.nodes.as_ref()?;
                            NodeList::from_str(nodes).ok()
                        });

                        (
                            Some(format!("MAC-VRF for vnet '{vnet_id}' (Remote {remote})")),
                            VnetStatusTable::new(remote.clone(), vnet_id.clone(), node_list).into(),
                        )
                    } else {
                        (None, html! {"Could not find vnet {vnet_id}!"})
                    }
                }
                DetailPanel::Zone {
                    remote,
                    zone: zone_id,
                } => {
                    let zone = self.zones.iter().find(|list_zone| {
                        list_zone.zone.zone.as_str() == zone_id.as_str()
                            && list_zone.remote.as_str() == remote.as_str()
                    });

                    let node_list = zone.as_ref().and_then(|zone| {
                        let nodes = zone.zone.nodes.as_ref()?;
                        NodeList::from_str(nodes).ok()
                    });

                    (
                        Some(format!("IP-VRF for zone '{zone_id}' (Remote {remote})")),
                        ZoneStatusTable::new(remote.clone(), zone_id.clone(), node_list).into(),
                    )
                }
            }
        } else {
            (
                None,
                Row::new()
                    .class(pwt::css::FlexFit)
                    .class(pwt::css::JustifyContent::Center)
                    .class(pwt::css::AlignItems::Center)
                    .with_child(html! { tr!("Select a Zone or VNet for more details.") })
                    .into(),
            )
        };

        let mut panel = Panel::new().width(600);

        if let Some(title) = title {
            panel.set_title(title);
        }

        panel.add_child(detail_html);
        container.add_child(panel);

        container.into()
    }

    fn dialog_view(
        &self,
        ctx: &LoadableComponentContext<Self>,
        view_state: &Self::ViewState,
    ) -> Option<Html> {
        let scope = ctx.link().clone();

        let on_success = Callback::from(move |upid: String| {
            scope.show_task_log(upid, None);
        });

        let on_close = ctx.link().clone().change_view_callback(|_| None);

        Some(match view_state {
            EvpnPanelViewState::AddZone => {
                AddZoneWindow::new(self.controllers.clone(), on_success, on_close).into()
            }
            EvpnPanelViewState::AddVnet => {
                AddVnetWindow::new(self.zones.clone(), on_success, on_close).into()
            }
        })
    }
}
