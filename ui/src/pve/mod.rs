use std::rc::Rc;

use yew::prelude::Component;
use yew::prelude::Context as YewContext;
use yew::prelude::Properties;
use yew::prelude::{html, Html};
use yew::virtual_dom::VComp;
use yew::virtual_dom::VNode;

use proxmox_rrd_api_types::{RrdMode, RrdTimeframe};
use proxmox_yew_comp::{RRDGraph, Series};
use pwt::props::{ContainerBuilder, CssBorderBuilder, CssPaddingBuilder, WidgetBuilder};

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

pub struct PveRemoteComp {
    nodes: Vec<String>,
    error: Option<String>,
}

pub enum Msg {
    NodeList(Vec<String>),
    Error(String),
}

impl Component for PveRemoteComp {
    type Message = Msg;
    type Properties = PveRemote;

    fn create(ctx: &YewContext<Self>) -> Self {
        let remote = ctx.props().remote.clone();

        ctx.link().send_future(Box::pin(async move {
            match crate::pdm_client().pve_list_nodes(&remote).await {
                Ok(nodes) => {
                    Msg::NodeList(nodes.into_iter().map(|response| response.node).collect())
                }
                Err(err) => Msg::Error(err.to_string()),
            }
        }));

        Self {
            nodes: Vec::new(),
            error: None,
        }
    }

    fn update(&mut self, _ctx: &YewContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Error(err) => self.error = Some(err),
            Msg::NodeList(nodes) => self.nodes = nodes,
        }
        true
    }

    fn view(&self, ctx: &YewContext<Self>) -> Html {
        if self.nodes.is_empty() {
            return html! { "Querying nodes..." };
        }

        let props = ctx.props();

        let mut view = pwt::widget::Column::new().padding(2);
        for node in &self.nodes {
            view.add_child(PveNodeRrd::new(props.remote.clone(), node.clone()));
        }

        pwt::widget::Panel::new()
            .class("pwt-flex-fit")
            .border(false)
            .title(props.remote.clone())
            .with_child(view)
            .into()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Properties)]
pub struct PveNodeRrd {
    remote: String,
    node: String,
}

impl PveNodeRrd {
    pub fn new(remote: String, node: String) -> Self {
        yew::props!(Self { remote, node })
    }
}

impl Into<VNode> for PveNodeRrd {
    fn into(self) -> VNode {
        VComp::new::<PveNodeRrdComp>(Rc::new(self), None).into()
    }
}

#[derive(Default)]
pub struct PveNodeRrdComp {
    error: Option<String>,
    time_data: Rc<Vec<i64>>,
    cpu_data: Option<Rc<Series>>,
    update_timer: Option<gloo_timers::callback::Timeout>,
}

pub enum PveNodeRrdMsg {
    Reload,
    Values(Vec<pdm_api_types::rrddata::NodeDataPoint>),
    Error,
}

impl PveNodeRrdComp {
    async fn reload(props: PveNodeRrd) -> PveNodeRrdMsg {
        match crate::pdm_client()
            .pve_node_rrddata(
                &props.remote,
                &props.node,
                RrdMode::Average,
                RrdTimeframe::Day,
            )
            .await
        {
            Ok(v) => PveNodeRrdMsg::Values(v),
            Err(_err) => PveNodeRrdMsg::Error,
        }
    }
}

impl Component for PveNodeRrdComp {
    type Message = PveNodeRrdMsg;
    type Properties = PveNodeRrd;

    fn create(ctx: &YewContext<Self>) -> Self {
        ctx.link().send_message(PveNodeRrdMsg::Reload);
        Self::default()
    }

    fn update(&mut self, ctx: &YewContext<Self>, msg: Self::Message) -> bool {
        match msg {
            PveNodeRrdMsg::Error => false,
            PveNodeRrdMsg::Reload => {
                ctx.link().send_future(Self::reload(ctx.props().clone()));
                false
            }
            PveNodeRrdMsg::Values(data) => {
                self.time_data = Rc::new(data.iter().map(|point| point.time as i64).collect());
                self.cpu_data = Some(Rc::new(Series::new(
                    "cpu",
                    data.iter()
                        .map(|point| point.cpu_current.unwrap_or_default())
                        .collect(),
                )));
                true
            }
        }
    }

    fn view(&self, _ctx: &YewContext<Self>) -> Html {
        let mut graph = RRDGraph::new(Rc::clone(&self.time_data)).title("Stuff");
        if let Some(values) = self.cpu_data.clone() {
            graph = graph.serie0(Some(values));
        }
        graph.into()
    }
}
