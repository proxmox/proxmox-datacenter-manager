use std::rc::Rc;

use yew::Properties;

use pwt::{
    css::AlignItems,
    prelude::*,
    props::WidgetBuilder,
    widget::{Fa, Panel, Row},
};
use pwt_macros::widget;

use pdm_api_types::resource::PveResource;

#[widget(comp=RemotePanelComp, @element)]
#[derive(Clone, Debug, PartialEq, Properties)]
pub struct RemotePanel {
    remote: String,
    resources: Rc<Vec<PveResource>>,
    error: Option<String>,
}

impl RemotePanel {
    pub fn new(remote: String, resources: Rc<Vec<PveResource>>, error: Option<String>) -> Self {
        yew::props!(Self {
            remote,
            resources,
            error
        })
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
            .with_child(tr! {"Remote '{0}'", ctx.props().remote})
            .into();
        Panel::new()
            .with_std_props(&ctx.props().std_props)
            .title(title)
            .into()
    }
}
