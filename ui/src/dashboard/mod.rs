use std::rc::Rc;

use yew::{
    virtual_dom::{VComp, VNode},
    Component,
};

use pwt::{
    css::FlexFit,
    prelude::*,
    widget::{Column, Row},
};

mod resource_status;
use resource_status::ResourceStatusPanel;

#[derive(Properties, PartialEq)]
pub struct Dashboard;

impl Dashboard {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for Dashboard {
    fn default() -> Self {
        Self::new()
    }
}

pub enum Msg {}

pub struct PdmDashboard {}

impl Component for PdmDashboard {
    type Message = Msg;
    type Properties = Dashboard;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self {}
    }

    fn view(&self, _ctx: &yew::Context<Self>) -> yew::Html {
        let content = Column::new()
            .class(FlexFit)
            .padding(4)
            .gap(2)
            .with_child(Row::new().with_child(ResourceStatusPanel::new()));

        content.into()
    }
}

impl From<Dashboard> for VNode {
    fn from(val: Dashboard) -> Self {
        let comp = VComp::new::<PdmDashboard>(Rc::new(val), None);
        VNode::from(comp)
    }
}
