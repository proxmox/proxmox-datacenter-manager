use std::rc::Rc;

use yew::{
    virtual_dom::{VComp, VNode},
    Context,
};

use proxmox_human_byte::HumanByte;
use proxmox_rrd_api_types::{RrdMode, RrdTimeframe};
use proxmox_yew_comp::{RRDGraph, Series};
use pwt::{
    css::{AlignItems, ColorScheme, FlexFit},
    prelude::*,
    props::{ContainerBuilder, WidgetBuilder},
    widget::{error_message, Column, Container, Fa, Panel, Progress, Row},
    AsyncPool,
};

use pdm_api_types::rrddata::NodeDataPoint;
use pdm_client::types::NodeStatus;

use crate::renderer::separator;

#[derive(Clone, Debug, Eq, PartialEq, Properties)]
pub struct NodePanel {
    /// The remote to show
    pub remote: String,

    /// The node to show
    pub node: String,

    #[prop_or(60_000)]
    /// The interval for refreshing the rrd data
    pub rrd_interval: u32,

    #[prop_or(10_000)]
    /// The interval for refreshing the status data
    pub status_interval: u32,
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
    ReloadStatus,
    LoadFinished(Result<Vec<NodeDataPoint>, proxmox_client::Error>),
    StatusLoadFinished(Result<NodeStatus, proxmox_client::Error>),
}

pub struct NodePanelComp {
    time_data: Rc<Vec<i64>>,
    cpu_data: Rc<Series>,
    load_data: Rc<Series>,
    mem_data: Rc<Series>,
    mem_total_data: Rc<Series>,
    status: Option<NodeStatus>,

    last_error: Option<proxmox_client::Error>,
    last_status_error: Option<proxmox_client::Error>,

    async_pool: AsyncPool,
    _timeout: Option<gloo_timers::callback::Timeout>,
    _status_timeout: Option<gloo_timers::callback::Timeout>,
}

impl NodePanelComp {
    async fn reload(remote: &str, node: &str) -> Msg {
        let res = crate::pdm_client()
            .pve_node_rrddata(remote, node, RrdMode::Average, RrdTimeframe::Hour)
            .await;

        Msg::LoadFinished(res)
    }

    async fn reload_status(remote: &str, node: &str) -> Result<NodeStatus, proxmox_client::Error> {
        let status = crate::pdm_client().pve_node_status(remote, node).await?;
        Ok(status)
    }
}

impl yew::Component for NodePanelComp {
    type Message = Msg;
    type Properties = NodePanel;

    fn create(ctx: &yew::Context<Self>) -> Self {
        ctx.link().send_message(Msg::Reload);
        ctx.link().send_message(Msg::ReloadStatus);
        Self {
            time_data: Rc::new(Vec::new()),
            cpu_data: Rc::new(Series::new("", Vec::new())),
            load_data: Rc::new(Series::new("", Vec::new())),
            mem_data: Rc::new(Series::new("", Vec::new())),
            mem_total_data: Rc::new(Series::new("", Vec::new())),
            status: None,
            last_error: None,
            last_status_error: None,
            async_pool: AsyncPool::new(),
            _timeout: None,
            _status_timeout: None,
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
            Msg::ReloadStatus => {
                self._status_timeout = None;
                let props = ctx.props();
                let remote = props.remote.clone();
                let node = props.node.clone();
                self.async_pool.send_future(ctx.link().clone(), async move {
                    let res = Self::reload_status(&remote, &node).await;
                    Msg::StatusLoadFinished(res)
                });
            }
            Msg::LoadFinished(res) => match res {
                Ok(data_points) => {
                    self.last_error = None;
                    let mut cpu_vec = Vec::with_capacity(data_points.len());
                    let mut load_vec = Vec::with_capacity(data_points.len());
                    let mut mem_vec = Vec::with_capacity(data_points.len());
                    let mut mem_total_vec = Vec::with_capacity(data_points.len());
                    let mut time_vec = Vec::with_capacity(data_points.len());
                    for data in data_points {
                        cpu_vec.push(data.cpu_current.unwrap_or(f64::NAN));
                        load_vec.push(data.cpu_avg1.unwrap_or(f64::NAN));
                        mem_vec.push(data.mem_used.unwrap_or(f64::NAN));
                        mem_total_vec.push(data.mem_total.unwrap_or(f64::NAN));
                        time_vec.push(data.time as i64);
                    }

                    self.cpu_data = Rc::new(Series::new(tr!("CPU"), cpu_vec));
                    self.load_data = Rc::new(Series::new(tr!("Server Load"), load_vec));
                    self.mem_data = Rc::new(Series::new(tr!("Used Memory"), mem_vec));
                    self.mem_total_data = Rc::new(Series::new(tr!("Total Memory"), mem_total_vec));
                    self.time_data = Rc::new(time_vec);

                    let link = ctx.link().clone();
                    self._timeout = Some(gloo_timers::callback::Timeout::new(
                        ctx.props().rrd_interval,
                        move || link.send_message(Msg::Reload),
                    ))
                }
                Err(err) => self.last_error = Some(err),
            },

            Msg::StatusLoadFinished(res) => {
                match res {
                    Ok(status) => {
                        self.last_status_error = None;
                        self.status = Some(status);
                    }
                    Err(err) => self.last_status_error = Some(err),
                }
                let link = ctx.link().clone();
                self._status_timeout = Some(gloo_timers::callback::Timeout::new(
                    ctx.props().status_interval,
                    move || link.send_message(Msg::ReloadStatus),
                ))
            }
        }
        true
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        if props.remote != old_props.remote || props.node != old_props.node {
            self.status = None;
            self.last_status_error = None;
            self.last_error = None;
            self.time_data = Rc::new(Vec::new());
            self.cpu_data = Rc::new(Series::new("", Vec::new()));
            self.load_data = Rc::new(Series::new("", Vec::new()));
            self.mem_data = Rc::new(Series::new("", Vec::new()));
            self.mem_total_data = Rc::new(Series::new("", Vec::new()));
            ctx.link()
                .send_message_batch(vec![Msg::Reload, Msg::ReloadStatus]);
            true
        } else {
            false
        }
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();
        let title: Html = Row::new()
            .gap(2)
            .class(AlignItems::Baseline)
            .with_child(Fa::new("building"))
            .with_child(tr! {"node '{0}'", props.node})
            .into();

        let mut status_comp = Column::new().gap(2).padding(4);
        if self.status.is_none() && self.last_status_error.is_none() {
            status_comp.add_child(Progress::new());
        }
        if let Some(status) = &self.status {
            let cpu = status.cpu;
            let maxcpu = status.cpuinfo.cpus;
            let load = status.loadavg.join(", ");

            let memory = status.memory.used as u64;
            let maxmem = status.memory.total as u64;
            let memory_used = memory as f64 / maxmem as f64;

            let root = status.rootfs.used as u64;
            let maxroot = status.rootfs.total as u64;
            let root_used = root as f64 / maxroot as f64;

            status_comp = status_comp
                .with_child(make_row(
                    tr!("CPU usage"),
                    Fa::new("database"),
                    tr!("{0}% of {1} CPU(s)", format!("{:.2}", cpu * 100.0), maxcpu),
                    Some(cpu as f32),
                ))
                .with_child(make_row(
                    tr!("Load average"),
                    Fa::new("line-chart"),
                    load,
                    None,
                ))
                .with_child(make_row(
                    tr!("Memory usage"),
                    Fa::new("list"),
                    tr!(
                        "{0}% ({1} of {2})",
                        format!("{:.2}", memory_used * 100.0),
                        HumanByte::from(memory),
                        HumanByte::from(maxmem),
                    ),
                    Some(memory_used as f32),
                ))
                .with_child(make_row(
                    tr!("Root filesystem usage"),
                    Fa::new("server"),
                    tr!(
                        "{0}% ({1} of {2})",
                        format!("{:.2}", root_used * 100.0),
                        HumanByte::from(root),
                        HumanByte::from(maxmem),
                    ),
                    Some(memory_used as f32),
                ))
                .with_child(Container::new().padding(1)) // spacer
                .with_child(
                    Row::new()
                        .with_child(tr!("Version"))
                        .with_flex_spacer()
                        .with_child(&status.pveversion),
                )
                .with_child(
                    Row::new()
                        .with_child(tr!("CPU Model"))
                        .with_flex_spacer()
                        .with_child(tr!(
                            "{0} ({1} sockets)",
                            status.cpuinfo.model,
                            status.cpuinfo.sockets
                        )),
                );
        }
        if let Some(err) = &self.last_status_error {
            status_comp.add_child(error_message(&err.to_string()));
        }

        Panel::new()
            .class(FlexFit)
            .title(title)
            .class(ColorScheme::Neutral)
            .with_child(status_comp)
            .with_child(separator().padding_x(4))
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
                                .title(tr!("Server load"))
                                .render_value(|v: &f64| {
                                    if v.is_finite() {
                                        format!("{:.2}", v)
                                    } else {
                                        v.to_string()
                                    }
                                })
                                .serie0(Some(self.load_data.clone())),
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

fn make_row(title: String, icon: Fa, text: String, meter_value: Option<f32>) -> Column {
    crate::renderer::status_row(title, icon, text, meter_value, false)
}
