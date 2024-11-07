use std::rc::Rc;

use yew::{
    virtual_dom::{VComp, VNode},
    Context,
};

use proxmox_yew_comp::{RRDGraph, Series};
use pwt::{
    css::FlexFit,
    prelude::*,
    props::{ContainerBuilder, WidgetBuilder},
    widget::{Column, Container, Panel},
    AsyncPool,
};

use pdm_api_types::rrddata::NodeDataPoint;
use proxmox_rrd_api_types::{RrdMode, RrdTimeframe};

#[derive(Clone, Debug, Eq, PartialEq, Properties)]
pub struct NodePanel {
    /// The remote to show
    pub remote: String,

    /// The node to show
    pub node: String,

    #[prop_or(60_000)]
    /// The interval for refreshing the data
    pub interval: u32,
}

impl NodePanel {
    pub fn new(remote: String, node: String) -> Self {
        yew::props!(Self { remote, node })
    }
}

impl Into<VNode> for NodePanel {
    fn into(self) -> VNode {
        VComp::new::<NodePanelComp>(Rc::new(self), None).into()
    }
}

pub enum Msg {
    Reload,
    LoadFinished(Result<Vec<NodeDataPoint>, proxmox_client::Error>),
}

pub struct NodePanelComp {
    time_data: Rc<Vec<i64>>,
    cpu_data: Rc<Series>,
    mem_data: Rc<Series>,
    mem_total_data: Rc<Series>,
    last_error: Option<proxmox_client::Error>,
    async_pool: AsyncPool,
    _timeout: Option<gloo_timers::callback::Timeout>,
}

impl NodePanelComp {
    async fn reload(remote: &str, node: &str) -> Msg {
        let res = crate::pdm_client()
            .pve_node_rrddata(remote, node, RrdMode::Average, RrdTimeframe::Hour)
            .await;

        Msg::LoadFinished(res)
    }
}

impl yew::Component for NodePanelComp {
    type Message = Msg;
    type Properties = NodePanel;

    fn create(ctx: &yew::Context<Self>) -> Self {
        ctx.link().send_message(Msg::Reload);
        Self {
            time_data: Rc::new(Vec::new()),
            cpu_data: Rc::new(Series::new("", Vec::new())),
            mem_data: Rc::new(Series::new("", Vec::new())),
            mem_total_data: Rc::new(Series::new("", Vec::new())),
            last_error: None,
            async_pool: AsyncPool::new(),
            _timeout: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Reload => {
                self._timeout = None;
                let props = ctx.props();
                let remote = props.remote.clone();
                let node = props.node.clone();
                self.async_pool.send_future(ctx.link().clone(), async move {
                    Self::reload(&remote, &node).await
                });
            }
            Msg::LoadFinished(res) => match res {
                Ok(data_points) => {
                    self.last_error = None;
                    let mut cpu_vec = Vec::with_capacity(data_points.len());
                    let mut mem_vec = Vec::with_capacity(data_points.len());
                    let mut mem_total_vec = Vec::with_capacity(data_points.len());
                    let mut time_vec = Vec::with_capacity(data_points.len());
                    for data in data_points {
                        cpu_vec.push(data.cpu_current.unwrap_or(f64::NAN));
                        mem_vec.push(data.mem_used.unwrap_or(f64::NAN));
                        mem_total_vec.push(data.mem_total.unwrap_or(f64::NAN));
                        time_vec.push(data.time as i64);
                    }

                    self.cpu_data = Rc::new(Series::new(tr!("CPU"), cpu_vec));
                    self.mem_data = Rc::new(Series::new(tr!("Used Memory"), mem_vec));
                    self.mem_total_data = Rc::new(Series::new(tr!("Total Memory"), mem_total_vec));
                    self.time_data = Rc::new(time_vec);

                    let link = ctx.link().clone();
                    self._timeout = Some(gloo_timers::callback::Timeout::new(
                        ctx.props().interval,
                        move || link.send_message(Msg::Reload),
                    ))
                }
                Err(err) => self.last_error = Some(err),
            },
        }
        true
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        if props.remote != old_props.remote || props.node != old_props.node {
            let remote = props.remote.clone();
            let node = props.node.clone();
            self.async_pool.send_future(ctx.link().clone(), async move {
                Self::reload(&remote, &node).await
            });
            true
        } else {
            false
        }
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();
        Panel::new()
            .class(FlexFit)
            .title(props.node.as_str())
            .with_child(
                Container::new().class(FlexFit).with_child(
                    Column::new()
                        .padding(4)
                        .gap(4)
                        .with_child(
                            RRDGraph::new(self.time_data.clone())
                                .title(tr!("CPU Usage"))
                                .render_value(|v: &f64| {
                                    if v.is_finite() {
                                        format!("{:.2}%", v * 100.0)
                                    } else {
                                        v.to_string()
                                    }
                                })
                                .serie0(Some(self.cpu_data.clone())),
                        )
                        .with_child(
                            RRDGraph::new(self.time_data.clone())
                                .title(tr!("Memory Usage"))
                                .binary(true)
                                .render_value(|v: &f64| {
                                    if v.is_finite() {
                                        proxmox_human_byte::HumanByte::from(*v as u64).to_string()
                                    } else {
                                        v.to_string()
                                    }
                                })
                                .serie0(Some(self.mem_total_data.clone()))
                                .serie1(Some(self.mem_data.clone())),
                        ),
                ),
            )
            .into()
    }
}
