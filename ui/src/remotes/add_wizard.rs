use std::rc::Rc;

use yew::html::IntoEventCallback;
use yew::virtual_dom::VComp;

use pwt::prelude::*;
use pwt::widget::TabBarItem;

use proxmox_yew_comp::{
    IntoSubmitValueCallback, SubmitValueCallback, Wizard, WizardPageRenderInfo,
};
use yew::virtual_dom::VNode;

use super::{ServerInfo, WizardPageConnect, WizardPageNodes, WizardPageSummary};

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
}

impl AddWizard {
    pub fn new() -> Self {
        yew::props!(Self {})
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
    ServerChange(Option<ServerInfo>),
}
pub struct AddWizardState {
    server_info: Option<ServerInfo>,
}

impl Component for AddWizardState {
    type Message = Msg;
    type Properties = AddWizard;

    fn create(_ctx: &Context<Self>) -> Self {
        Self { server_info: None }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::ServerChange(server_info) => {
                self.server_info = server_info;
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        Wizard::new("Add Remote")
            .tab_bar_style(pwt::widget::TabBarStyle::MaterialPrimary)
            //.width(720)
            //.height(300)
            .on_done(props.on_close.clone())
            .with_page(TabBarItem::new().label(tr!("Address")), {
                let link = ctx.link().clone();
                move |p: &WizardPageRenderInfo| {
                    WizardPageConnect::new(p.clone())
                        .on_server_change(link.callback(Msg::ServerChange))
                        .into()
                }
            })
            .with_page(TabBarItem::new().label(tr!("Nodes")), {
                let server_info = self.server_info.clone();
                move |p: &WizardPageRenderInfo| {
                    WizardPageNodes::new(p.clone())
                        .server_info(server_info.clone())
                        .into()
                }
            })
            .with_page(TabBarItem::new().label(tr!("Summary")), {
                let server_info = self.server_info.clone();
                move |p: &WizardPageRenderInfo| {
                    WizardPageSummary::new(p.clone())
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
