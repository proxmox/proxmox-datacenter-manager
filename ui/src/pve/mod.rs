use std::rc::Rc;

use yew::{
    prelude::Html,
    virtual_dom::{VComp, VNode},
};

use pwt::prelude::*;
use pwt::props::{ContainerBuilder, WidgetBuilder};
use pwt::widget::{Pane, SplitPane};
use pwt::{css::FlexFit, state::NavigationContainer};

pub mod lxc;
pub mod node;
pub mod qemu;
pub mod remote;
pub mod utils;

mod tree;
use tree::PveTreeNode;

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
        VComp::new::<PveRemoteComp>(Rc::new(self), None).into()
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
}

pub struct PveRemoteComp {
    view: tree::PveTreeNode,
}

impl Component for PveRemoteComp {
    type Message = Msg;
    type Properties = PveRemote;

    fn create(_ctx: &yew::Context<PveRemoteComp>) -> Self {
        Self {
            view: PveTreeNode::Root(false),
        }
    }

    fn update(&mut self, _ctx: &yew::Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::SelectedView(node) => {
                self.view = node;
            }
        }
        true
    }

    fn view(&self, ctx: &yew::Context<Self>) -> Html {
        let props = ctx.props();

        let remote = &props.remote;

        let content: Html = match &self.view {
            PveTreeNode::Root(_) => remote::RemotePanel::new(remote.clone()).into(),
            PveTreeNode::Node(node) => {
                node::NodePanel::new(remote.clone(), node.node.clone()).into()
            }
            PveTreeNode::Qemu(qemu) => {
                qemu::QemuPanel::new(remote.clone(), qemu.node.clone(), qemu.clone()).into()
            }
            PveTreeNode::Lxc(lxc) => {
                lxc::LxcPanel::new(remote.clone(), lxc.node.clone(), lxc.clone()).into()
            }
        };

        NavigationContainer::new()
            .with_child(
                SplitPane::new()
                    .class(FlexFit)
                    .with_child(
                        Pane::new(tree::PveTree::new(
                            remote.to_string(),
                            ctx.link().callback(|node| Msg::SelectedView(node)),
                        ))
                        .min_size(400)
                        .flex(1),
                    )
                    .with_child(Pane::new(content).flex(1)),
            )
            .into()
    }
}
