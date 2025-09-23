use std::{fmt::Display, rc::Rc};

use gloo_utils::window;
use proxmox_client::Error;
use proxmox_yew_comp::{LoadableComponent, LoadableComponentContext, LoadableComponentMaster};
use yew::{
    prelude::Html,
    virtual_dom::{VComp, VNode},
};

use pwt::{
    css::AlignItems,
    state::NavigationContainer,
    widget::{Button, Container, Fa},
};
use pwt::{
    css::FlexFit,
    prelude::*,
    props::{ContainerBuilder, WidgetBuilder},
    widget::{Column, Panel, Row},
};

use pdm_api_types::resource::{PveResource, ResourceType};

pub mod lxc;
pub mod node;
pub mod qemu;
pub mod remote;
pub mod storage;
pub mod utils;

mod tree;
use tree::PveTreeNode;

use crate::{get_deep_url, remotes::RemoteTaskList};

#[derive(Debug, Eq, PartialEq, Properties)]
pub struct PveRemote {
    remote: String,
}

impl PveRemote {
    pub fn new(remote: String) -> Self {
        yew::props!(Self { remote })
    }
}

impl Into<VNode> for PveRemote {
    fn into(self) -> VNode {
        VComp::new::<LoadableComponentMaster<PveRemoteComp>>(Rc::new(self), None).into()
    }
}

#[derive(PartialEq, Clone)]
pub enum Action {
    Start,
    Shutdown,
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Action::Start => tr!("Start"),
            Action::Shutdown => tr!("Shutdown"),
        };
        f.write_str(&text)
    }
}

#[derive(PartialEq, Clone, Copy)]
pub enum GuestType {
    Qemu,
    Lxc,
}

impl Display for GuestType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GuestType::Qemu => f.write_str("qemu"),
            GuestType::Lxc => f.write_str("lxc"),
        }
    }
}

impl From<GuestType> for ResourceType {
    fn from(value: GuestType) -> Self {
        match value {
            GuestType::Qemu => ResourceType::PveQemu,
            GuestType::Lxc => ResourceType::PveLxc,
        }
    }
}

impl From<GuestType> for Fa {
    fn from(val: GuestType) -> Self {
        let icon = match val {
            GuestType::Qemu => "desktop",
            GuestType::Lxc => "cubes",
        };
        Fa::new(icon)
    }
}

#[derive(PartialEq, Clone, Copy)]
pub struct GuestInfo {
    pub guest_type: GuestType,
    pub vmid: u32,
}

impl GuestInfo {
    fn new(guest_type: GuestType, vmid: u32) -> Self {
        Self { guest_type, vmid }
    }

    fn local_id(&self) -> String {
        match self.guest_type {
            GuestType::Qemu => format!("qemu/{}", self.vmid),
            GuestType::Lxc => format!("lxc/{}", self.vmid),
        }
    }
}

pub enum Msg {
    SelectedView(tree::PveTreeNode),
    ResourcesList(Result<Vec<PveResource>, Error>),
}

pub struct PveRemoteComp {
    view: tree::PveTreeNode,
    resources: Rc<Vec<PveResource>>,
    last_error: Option<String>,
}

impl LoadableComponent for PveRemoteComp {
    type Message = Msg;
    type Properties = PveRemote;
    type ViewState = ();

    fn create(ctx: &LoadableComponentContext<PveRemoteComp>) -> Self {
        ctx.link().repeated_load(5000);
        Self {
            view: PveTreeNode::Root,
            resources: Rc::new(Vec::new()),
            last_error: None,
        }
    }

    fn update(&mut self, _ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::SelectedView(node) => {
                self.view = node;
            }
            Msg::ResourcesList(res) => match res {
                Ok(res) => {
                    self.last_error = None;
                    self.resources = Rc::new(res);
                }
                Err(err) => {
                    self.last_error = Some(err.to_string());
                    _ctx.link()
                        .show_error(tr!("Load failed"), err.to_string(), false);
                }
            },
        }
        true
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        let props = ctx.props();

        let remote = &props.remote;

        let content: Html = match &self.view {
            PveTreeNode::Root => Panel::new()
                .title(tr!("Remote Tasks"))
                .with_child(RemoteTaskList::new().remote(remote.clone()))
                .into(),
            PveTreeNode::Node(node) => {
                node::NodePanel::new(remote.clone(), node.node.clone()).into()
            }
            PveTreeNode::Qemu(qemu) => {
                qemu::QemuPanel::new(remote.clone(), qemu.node.clone(), qemu.clone()).into()
            }
            PveTreeNode::Lxc(lxc) => {
                lxc::LxcPanel::new(remote.clone(), lxc.node.clone(), lxc.clone()).into()
            }
            PveTreeNode::Storage(storage) => {
                storage::StoragePanel::new(remote.clone(), storage.node.clone(), storage.clone())
                    .into()
            }
        };

        let link = ctx.link();

        let title: Html = Row::new()
            .gap(2)
            .class(AlignItems::Center)
            .with_child(Fa::new("server"))
            .with_child(tr! {"Remote '{0}'", ctx.props().remote})
            .into();

        let content = Container::new()
            .class("pwt-content-spacer")
            .class(FlexFit)
            .class("pwt-flex-direction-row")
            .with_child(
                Panel::new()
                    .min_width(500)
                    .style("flex", "1 1 0")
                    .class(FlexFit)
                    .border(true)
                    .title(title)
                    .with_tool(
                        Button::new(tr!("Open Web UI"))
                            .icon_class("fa fa-external-link")
                            .on_activate({
                                let link = ctx.link().clone();
                                let remote = ctx.props().remote.clone();
                                move |_| {
                                    if let Some(url) =
                                        get_deep_url(&link.yew_link(), &remote, None, "")
                                    {
                                        let _ = window().open_with_url(&url.href());
                                    }
                                }
                            }),
                    )
                    .with_child(
                        Column::new()
                            .padding(4)
                            .class(FlexFit)
                            .gap(4)
                            .with_child(remote::RemotePanel::new(
                                remote.clone(),
                                self.resources.clone(),
                                self.last_error.clone(),
                            ))
                            .with_child(html! {<hr/>})
                            .with_child(tree::PveTree::new(
                                remote.to_string(),
                                self.resources.clone(),
                                ctx.loading(),
                                link.callback(Msg::SelectedView),
                                {
                                    let link = link.clone();
                                    move |_| link.send_reload()
                                },
                            )),
                    ),
            )
            .with_child(
                Panel::new()
                    .class(FlexFit)
                    .border(true)
                    .min_width(500)
                    .with_child(content)
                    .style("flex", "1 1 0"),
            );
        NavigationContainer::new()
            .with_child(Panel::new().class(FlexFit).with_child(content))
            .into()
    }

    fn load(
        &self,
        ctx: &LoadableComponentContext<Self>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), anyhow::Error>>>> {
        let link = ctx.link();
        let remote = ctx.props().remote.clone();
        Box::pin(async move {
            let res = crate::pdm_client()
                .pve_cluster_resources(&remote, None)
                .await;
            link.send_message(Msg::ResourcesList(res));
            Ok(())
        })
    }
}
