use futures::try_join;
use std::rc::Rc;

use anyhow::Error;
use yew::virtual_dom::{VComp, VNode};
use yew::{html, Callback, Html, Properties};

use pdm_client::types::{ListController, ListControllersType, ListVnet, ListZone, ListZonesType};
use proxmox_yew_comp::{LoadableComponent, LoadableComponentContext, LoadableComponentMaster};

use pwt::css::{AlignItems, FlexFit, JustifyContent};
use pwt::props::{ContainerBuilder, EventSubscriber, StorageLocation, WidgetBuilder};
use pwt::state::NavigationContainer;
use pwt::tr;
use pwt::widget::menu::{Menu, MenuButton, MenuItem};
use pwt::widget::{Button, Column, MiniScrollMode, TabBarItem, TabPanel, Toolbar};

use crate::pdm_client;
use crate::sdn::evpn::{AddVnetWindow, AddZoneWindow, RemoteTree, VrfTree};

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

pub enum EvpnPanelMsg {
    Reload,
    LoadFinished {
        controllers: Rc<Vec<ListController>>,
        zones: Rc<Vec<ListZone>>,
        vnets: Rc<Vec<ListVnet>>,
    },
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
    controllers: Rc<Vec<ListController>>,
    zones: Rc<Vec<ListZone>>,
    vnets: Rc<Vec<ListVnet>>,
    initial_load: bool,
}

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
            .with_child(Button::refresh(ctx.loading()).onclick(on_refresh))
    }
}

impl LoadableComponent for EvpnPanelComponent {
    type Properties = EvpnPanel;
    type Message = EvpnPanelMsg;
    type ViewState = EvpnPanelViewState;

    fn create(_ctx: &LoadableComponentContext<Self>) -> Self {
        Self {
            initial_load: true,
            controllers: Default::default(),
            zones: Default::default(),
            vnets: Default::default(),
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
            Self::Message::Reload => {
                ctx.link().send_reload();
            }
        }

        false
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        let panel = TabPanel::new()
            .state_id(StorageLocation::session("EvpnPanelState"))
            .class(pwt::css::FlexFit)
            .router(true)
            .scroll_mode(MiniScrollMode::Arrow)
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
                        ))
                    }),
            );

        let navigation_container = NavigationContainer::new().with_child(panel);

        Column::new()
            .class(pwt::css::FlexFit)
            .with_child(navigation_container)
            .into()
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
