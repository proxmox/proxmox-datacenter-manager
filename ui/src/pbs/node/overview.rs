use std::rc::Rc;

use yew::{
    virtual_dom::{VComp, VNode},
    Context,
};

use proxmox_yew_comp::{node_info, RRDGraph, RRDTimeframe, RRDTimeframeSelector, Series};
use pwt::{
    css::{AlignItems, ColorScheme, FlexFit, JustifyContent},
    prelude::*,
    props::{ContainerBuilder, WidgetBuilder},
    widget::{error_message, Column, Container, Fa, Panel, Progress, Row},
    AsyncPool,
};

use pbs_api_types::NodeStatus;
use pdm_api_types::rrddata::PbsNodeDataPoint;

use crate::{renderer::separator, LoadResult};

#[derive(Clone, Debug, Eq, PartialEq, Properties)]
pub struct PbsNodeOverviewPanel {
    /// The remote to show
    pub remote: String,

    #[prop_or(60_000)]
    /// The interval for refreshing the rrd data
    pub rrd_interval: u32,

    #[prop_or(10_000)]
    /// The interval for refreshing the status data
    pub status_interval: u32,
}

impl PbsNodeOverviewPanel {
    pub fn new(remote: String) -> Self {
        yew::props!(Self { remote })
    }
}

impl From<PbsNodeOverviewPanel> for VNode {
    fn from(val: PbsNodeOverviewPanel) -> Self {
        VComp::new::<PbsNodeOverviewPanelComp>(Rc::new(val), None).into()
    }
}

enum Msg {
    ReloadRrd,
    ReloadStatus,
    LoadFinished(Result<Vec<PbsNodeDataPoint>, proxmox_client::Error>),
    StatusLoadFinished(Result<NodeStatus, proxmox_client::Error>),
    UpdateRrdTimeframe(RRDTimeframe),
}

struct PbsNodeOverviewPanelComp {
    time_data: Rc<Vec<i64>>,
    cpu_data: Rc<Series>,
    load_data: Rc<Series>,
    mem_data: Rc<Series>,
    mem_total_data: Rc<Series>,
    status: LoadResult<NodeStatus, proxmox_client::Error>,

    rrd_time_frame: RRDTimeframe,

    last_error: Option<proxmox_client::Error>,

    async_pool: AsyncPool,
    _timeout: Option<gloo_timers::callback::Timeout>,
    _status_timeout: Option<gloo_timers::callback::Timeout>,
}

impl PbsNodeOverviewPanelComp {
    async fn reload_rrd(remote: &str, rrd_time_frame: RRDTimeframe) -> Msg {
        let res = crate::pdm_client()
            .pbs_node_rrddata(remote, rrd_time_frame.mode, rrd_time_frame.timeframe)
            .await;

        Msg::LoadFinished(res)
    }

    async fn reload_status(remote: &str) -> Result<NodeStatus, proxmox_client::Error> {
        let status = crate::pdm_client().pbs_node_status(remote).await?;
        Ok(status)
    }
}

impl yew::Component for PbsNodeOverviewPanelComp {
    type Message = Msg;
    type Properties = PbsNodeOverviewPanel;

    fn create(ctx: &yew::Context<Self>) -> Self {
        ctx.link().send_message(Msg::ReloadRrd);
        ctx.link().send_message(Msg::ReloadStatus);
        Self {
            time_data: Rc::new(Vec::new()),
            cpu_data: Rc::new(Series::new("", Vec::new())),
            load_data: Rc::new(Series::new("", Vec::new())),
            mem_data: Rc::new(Series::new("", Vec::new())),
            mem_total_data: Rc::new(Series::new("", Vec::new())),
            rrd_time_frame: RRDTimeframe::load(),
            status: LoadResult::new(),
            last_error: None,
            async_pool: AsyncPool::new(),
            _timeout: None,
            _status_timeout: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::ReloadRrd => {
                self._timeout = None;
                let props = ctx.props();
                let remote = props.remote.clone();
                let timeframe = self.rrd_time_frame;
                self.async_pool.send_future(ctx.link().clone(), async move {
                    Self::reload_rrd(&remote, timeframe).await
                });
            }
            Msg::ReloadStatus => {
                self._status_timeout = None;
                let props = ctx.props();
                let remote = props.remote.clone();
                self.async_pool.send_future(ctx.link().clone(), async move {
                    let res = Self::reload_status(&remote).await;
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
                        move || link.send_message(Msg::ReloadRrd),
                    ))
                }
                Err(err) => self.last_error = Some(err),
            },
            Msg::StatusLoadFinished(res) => {
                self.status.update(res);
                let link = ctx.link().clone();
                self._status_timeout = Some(gloo_timers::callback::Timeout::new(
                    ctx.props().status_interval,
                    move || link.send_message(Msg::ReloadStatus),
                ))
            }
            Msg::UpdateRrdTimeframe(rrd_time_frame) => {
                self.rrd_time_frame = rrd_time_frame;
                ctx.link().send_message(Msg::ReloadRrd);
                return false;
            }
        }
        true
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        if props.remote != old_props.remote {
            self.status = LoadResult::new();
            self.last_error = None;
            self.time_data = Rc::new(Vec::new());
            self.cpu_data = Rc::new(Series::new("", Vec::new()));
            self.load_data = Rc::new(Series::new("", Vec::new()));
            self.mem_data = Rc::new(Series::new("", Vec::new()));
            self.mem_total_data = Rc::new(Series::new("", Vec::new()));
            self.async_pool = AsyncPool::new();
            ctx.link()
                .send_message_batch(vec![Msg::ReloadRrd, Msg::ReloadStatus]);
            true
        } else {
            false
        }
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let status_comp = node_info(self.status.data.as_ref().map(|s| s.into()));

        let title: Html = Row::new()
            .gap(2)
            .class(AlignItems::Baseline)
            .with_child(Fa::new("tachometer"))
            .with_child(tr! {"Overview"})
            .into();
        Panel::new()
            .title(title)
            .class(FlexFit)
            .class(ColorScheme::Neutral)
            .with_child(
                // FIXME: add some 'visible' or 'active' property to the progress
                Progress::new()
                    .value(self.status.has_data().then_some(0.0))
                    .style("opacity", self.status.has_data().then_some("0")),
            )
            .with_child(status_comp)
            .with_optional_child(
                self.status
                    .error
                    .as_ref()
                    .map(|err| error_message(&err.to_string())),
            )
            .with_child(separator().padding_x(4))
            .with_child(
                Row::new()
                    .padding_x(4)
                    .padding_y(1)
                    .class(JustifyContent::FlexEnd)
                    .with_child(
                        RRDTimeframeSelector::new()
                            .on_change(ctx.link().callback(Msg::UpdateRrdTimeframe)),
                    ),
            )
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
                                .serie0(Some(self.mem_data.clone()))
                                .serie1(Some(self.mem_total_data.clone())),
                        ),
                ),
            )
            .into()
    }
}
