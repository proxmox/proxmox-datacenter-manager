use std::rc::Rc;

use pdm_api_types::resource::PveQemuResource;
use yew::{
    virtual_dom::{VComp, VNode},
    Properties,
};

use pwt::{css::FlexFit, props::WidgetBuilder, widget::Panel};

#[derive(Clone, Debug, Properties)]
pub struct QemuPanel {
    remote: String,
    node: String,
    info: PveQemuResource,
}

impl PartialEq for QemuPanel {
    fn eq(&self, other: &Self) -> bool {
        if self.remote == other.remote && self.node == other.node {
            // only check some fields, so we don't update when e.g. only the cpu changes
            self.info.name == other.info.name
                && self.info.id == other.info.id
                && self.info.node == other.node
        } else {
            false
        }
    }
}
impl Eq for QemuPanel {}

impl QemuPanel {
    pub fn new(remote: String, node: String, info: PveQemuResource) -> Self {
        yew::props!(Self { remote, node, info })
    }
}

impl Into<VNode> for QemuPanel {
    fn into(self) -> VNode {
        VComp::new::<QemuPanelComp>(Rc::new(self), None).into()
    }
}

pub struct QemuPanelComp {}

impl yew::Component for QemuPanelComp {
    type Message = ();

    type Properties = QemuPanel;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self {}
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        Panel::new()
            .class(FlexFit)
            .title(crate::pve::utils::render_qemu_name(&ctx.props().info, true))
            .into()
    }
}
