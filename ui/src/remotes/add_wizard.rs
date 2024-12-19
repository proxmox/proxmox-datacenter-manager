use std::rc::Rc;

use yew::html::IntoEventCallback;
use yew::virtual_dom::VComp;

use pwt::prelude::*;
use pwt::widget::TabBarItem;

use pdm_api_types::remotes::{Remote, RemoteType};

use proxmox_yew_comp::{
    IntoSubmitValueCallback, SubmitValueCallback, Wizard, WizardPageRenderInfo,
};
use yew::virtual_dom::VNode;

use super::{
    wizard_page_connect::ConnectParams, WizardPageConnect, WizardPageInfo, WizardPageNodes,
    WizardPageSummary,
};

use pwt_macros::builder;

#[derive(Properties, Clone, PartialEq)]
#[builder]
pub struct AddWizard {
    /// Dialog close callback.
    #[builder_cb(IntoEventCallback, into_event_callback, ())]
    #[prop_or_default]
    pub on_close: Option<Callback<()>>,

    /// Dialog submit callback.
    #[prop_or_default]
    pub on_submit: Option<SubmitValueCallback>,

    remote_type: RemoteType,
}

impl AddWizard {
    pub fn new(remote_type: RemoteType) -> Self {
        yew::props!(Self { remote_type })
    }

    /// Set [Self::on_submit] callback
    pub fn set_on_submit(&mut self, on_submit: impl IntoSubmitValueCallback) {
        self.on_submit = on_submit.into_submit_value_callback();
    }

    /// Builder style method to set [Self::on_submit] callback
    pub fn on_submit(mut self, on_submit: impl IntoSubmitValueCallback) -> Self {
        self.set_on_submit(on_submit);
        self
    }
}

pub enum Msg {
    ServerChange(Option<Remote>),
    ConnectChange(Option<ConnectParams>),
}
pub struct AddWizardState {
    server_info: Option<Remote>,
    connect_info: Option<ConnectParams>,
}

impl Component for AddWizardState {
    type Message = Msg;
    type Properties = AddWizard;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            server_info: None,
            connect_info: None,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::ServerChange(server_info) => {
                self.server_info = server_info;
            }
            Msg::ConnectChange(realms) => {
                self.connect_info = realms;
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let remote_type = props.remote_type;

        let mut wizard = Wizard::new("Add Remote")
            .width(800)
            .tab_bar_style(pwt::widget::TabBarStyle::MaterialPrimary)
            .on_done(props.on_close.clone())
            .with_page(
                TabBarItem::new()
                    .key("connection")
                    .label(if remote_type == RemoteType::Pve {
                        tr!("Probe Remote")
                    } else {
                        tr!("Connection")
                    }),
                {
                    let link = ctx.link().clone();
                    move |p: &WizardPageRenderInfo| {
                        WizardPageConnect::new(p.clone(), remote_type)
                            .on_connect_change(link.callback(Msg::ConnectChange))
                            .into()
                    }
                },
            )
            .with_page(TabBarItem::new().key("info").label(tr!("Settings")), {
                let realms = self.connect_info.clone();
                let link = ctx.link().clone();
                move |p: &WizardPageRenderInfo| {
                    WizardPageInfo::new(p.clone())
                        .connect_info(realms.clone())
                        .on_server_change(link.callback(Msg::ServerChange))
                        .into()
                }
            });

        if remote_type == RemoteType::Pve {
            wizard = wizard.with_page(TabBarItem::new().key("nodes").label(tr!("Endpoints")), {
                let server_info = self.server_info.clone();
                move |p: &WizardPageRenderInfo| {
                    WizardPageNodes::new(p.clone())
                        .server_info(server_info.clone())
                        .into()
                }
            });
        }

        wizard
            .with_page(TabBarItem::new().label(tr!("Summary")), {
                let server_info = self.server_info.clone();
                move |p: &WizardPageRenderInfo| {
                    WizardPageSummary::new(p.clone(), remote_type)
                        .server_info(server_info.clone())
                        .into()
                }
            })
            .on_submit(props.on_submit.clone())
            .into()
    }
}

impl Into<VNode> for AddWizard {
    fn into(self) -> VNode {
        let comp = VComp::new::<AddWizardState>(Rc::new(self), None);
        VNode::from(comp)
    }
}
