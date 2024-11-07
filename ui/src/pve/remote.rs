use std::rc::Rc;

use yew::{
    virtual_dom::{VComp, VNode},
    Properties,
};

use pwt::{
    css::{AlignItems, FlexFit},
    prelude::*,
    props::WidgetBuilder,
    widget::{Fa, Panel, Row},
};

#[derive(Clone, Debug, Eq, PartialEq, Properties)]
pub struct RemotePanel {
    remote: String,
}

impl RemotePanel {
    pub fn new(remote: String) -> Self {
        yew::props!(Self { remote })
    }
}

impl Into<VNode> for RemotePanel {
    fn into(self) -> VNode {
        VComp::new::<RemotePanelComp>(Rc::new(self), None).into()
    }
}

pub struct RemotePanelComp {}

impl yew::Component for RemotePanelComp {
    type Message = ();

    type Properties = RemotePanel;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self {}
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let title: Html = Row::new()
            .gap(2)
            .class(AlignItems::Center)
            .with_child(Fa::new("server"))
            .with_child(ctx.props().remote.as_str())
            .into();
        Panel::new().class(FlexFit).title(title).into()
    }
}
