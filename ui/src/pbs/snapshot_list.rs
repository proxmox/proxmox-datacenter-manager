//! Streaming snapshot listing.

use std::rc::Rc;

use yew::virtual_dom::{VComp, VNode};
use yew::Properties;

use pwt::prelude::{html, Component, Context, Html};

#[derive(Clone, PartialEq, Properties)]
pub struct SnapshotList {
    remote: String,
}

impl SnapshotList {
    pub fn new(remote: String) -> Self {
        yew::props!(Self { remote })
    }
}

impl Into<VNode> for SnapshotList {
    fn into(self) -> VNode {
        let comp = VComp::new::<SnapshotListComp>(Rc::new(self), None);
        VNode::from(comp)
    }
}

enum Msg {}

struct SnapshotListComp {}

impl Component for SnapshotListComp {
    type Message = Msg;
    type Properties = SnapshotList;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {}
    }

    fn update(&mut self, _ctx: &Context<Self>, _msg: Self::Message) -> bool {
        false
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! { format!("Showing pbs {remote}", remote = ctx.props().remote) }
    }
}
