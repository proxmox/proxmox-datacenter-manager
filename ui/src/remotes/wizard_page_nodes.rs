use std::rc::Rc;

use pdm_client::types::Remote;
use pwt::css::FlexFit;
use yew::virtual_dom::{VComp, VNode};

use pwt::prelude::*;
use pwt::widget::Container;

use proxmox_yew_comp::WizardPageRenderInfo;

use pwt_macros::builder;

use super::NodeUrlList;

#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct WizardPageNodes {
    info: WizardPageRenderInfo,

    #[builder]
    #[prop_or_default]
    server_info: Option<Remote>,
}

impl WizardPageNodes {
    pub fn new(info: WizardPageRenderInfo) -> Self {
        yew::props!(Self { info })
    }
}

pub struct PdmWizardPageNodes {}

impl Component for PdmWizardPageNodes {
    type Message = ();
    type Properties = WizardPageNodes;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {}
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let nodes = ctx
            .props()
            .server_info
            .as_ref()
            .map(|info| info.nodes.clone())
            .unwrap_or_default();
        Container::new()
            .class(FlexFit)
            .padding(4)
            .with_child(
                NodeUrlList::new()
                    .max_height(400)
                    .default(nodes)
                    .name("nodes")
                    .key("nodes")
                    .required(true),
            )
            .into()
    }
}

impl Into<VNode> for WizardPageNodes {
    fn into(self) -> VNode {
        let comp = VComp::new::<PdmWizardPageNodes>(Rc::new(self), None);
        VNode::from(comp)
    }
}
