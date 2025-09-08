use core::f64;
use std::rc::Rc;

use gloo_timers::callback::Timeout;
use serde_json::json;
use yew::{
    virtual_dom::{VComp, VNode},
    Properties,
};

use proxmox_human_byte::HumanByte;
use proxmox_yew_comp::{RRDGraph, RRDTimeframe, RRDTimeframeSelector, Series};
use pwt::{
    css::{AlignItems, ColorScheme, FlexFit, JustifyContent},
    prelude::*,
    props::WidgetBuilder,
    widget::{Column, Container, Fa, Panel, Progress, Row},
    AsyncPool,
};

use pdm_api_types::{resource::PveQemuResource, rrddata::QemuDataPoint};
use pdm_client::types::{IsRunning, QemuStatus};

use crate::{pve::utils::render_qemu_name, renderer::separator};

#[derive(Clone, Debug, Properties)]
pub struct QemuPanel {
    remote: String,
    node: String,
    info: PveQemuResource,

    #[prop_or(60_000)]
    /// The interval for refreshing the rrd data
    pub rrd_interval: u32,

    #[prop_or(10_000)]
    /// The interval for refreshing the status data
    pub status_interval: u32,
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

pub enum Msg {
    ReloadStatus,
    ReloadRrd,
    StatusResult(Result<QemuStatus, proxmox_client::Error>),
    RrdResult(Result<Vec<QemuDataPoint>, proxmox_client::Error>),
    UpdateRrdTimeframe(RRDTimeframe),
}

pub struct QemuPanelComp {
    status: Option<QemuStatus>,
    last_status_error: Option<proxmox_client::Error>,
    last_rrd_error: Option<proxmox_client::Error>,
    _status_timeout: Option<Timeout>,
    _rrd_timeout: Option<Timeout>,
    _async_pool: AsyncPool,

    rrd_time_frame: RRDTimeframe,

    time: Rc<Vec<i64>>,
    cpu: Rc<Series>,
    memory: Rc<Series>,
    memory_max: Rc<Series>,
    netin: Rc<Series>,
    netout: Rc<Series>,
    diskread: Rc<Series>,
    diskwrite: Rc<Series>,
}

impl QemuPanelComp {
    async fn reload_status(remote: &str, vmid: u32) -> Result<QemuStatus, proxmox_client::Error> {
        let status = crate::pdm_client()
            .pve_qemu_status(remote, None, vmid)
            .await?;
        Ok(status)
    }

    async fn reload_rrd(
        remote: &str,
        vmid: u32,
        rrd_time_frame: RRDTimeframe,
    ) -> Result<Vec<QemuDataPoint>, proxmox_client::Error> {
        let rrd = crate::pdm_client()
            .pve_qemu_rrddata(remote, vmid, rrd_time_frame.mode, rrd_time_frame.timeframe)
            .await?;
        Ok(rrd)
    }
}

impl yew::Component for QemuPanelComp {
    type Message = Msg;

    type Properties = QemuPanel;

    fn create(ctx: &yew::Context<Self>) -> Self {
        ctx.link()
            .send_message_batch(vec![Msg::ReloadStatus, Msg::ReloadRrd]);
        Self {
            status: None,
            _status_timeout: None,
            _rrd_timeout: None,
            _async_pool: AsyncPool::new(),
            last_rrd_error: None,
            last_status_error: None,

            rrd_time_frame: RRDTimeframe::load(),

            time: Rc::new(Vec::new()),
            cpu: Rc::new(Series::new("", Vec::new())),
            memory: Rc::new(Series::new("", Vec::new())),
            memory_max: Rc::new(Series::new("", Vec::new())),
            netin: Rc::new(Series::new("", Vec::new())),
            netout: Rc::new(Series::new("", Vec::new())),
            diskread: Rc::new(Series::new("", Vec::new())),
            diskwrite: Rc::new(Series::new("", Vec::new())),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let link = ctx.link().clone();
        let props = ctx.props();
        let remote = props.remote.clone();
        let vmid = props.info.vmid;
        match msg {
            Msg::ReloadStatus => {
                self._async_pool.send_future(link, async move {
                    Msg::StatusResult(Self::reload_status(&remote, vmid).await)
                });
                false
            }
            Msg::ReloadRrd => {
                let time_frame = self.rrd_time_frame;
                self._async_pool.send_future(link, async move {
                    Msg::RrdResult(Self::reload_rrd(&remote, vmid, time_frame).await)
                });
                false
            }
            Msg::StatusResult(res) => {
                match res {
                    Ok(status) => {
                        self.last_status_error = None;
                        self.status = Some(status);
                    }
                    Err(err) => {
                        self.last_status_error = Some(err);
                    }
                }

                self._status_timeout = Some(Timeout::new(props.status_interval, move || {
                    link.send_message(Msg::ReloadStatus)
                }));
                true
            }
            Msg::RrdResult(res) => {
                match res {
                    Ok(rrd) => {
                        self.last_rrd_error = None;

                        let mut cpu = Vec::new();
                        let mut memory = Vec::new();
                        let mut memory_max = Vec::new();
                        let mut netin = Vec::new();
                        let mut netout = Vec::new();
                        let mut diskread = Vec::new();
                        let mut diskwrite = Vec::new();
                        let mut time = Vec::new();
                        for data in rrd {
                            cpu.push(data.cpu_current.unwrap_or(f64::NAN));
                            memory.push(data.mem_used.unwrap_or(f64::NAN));
                            memory_max.push(data.mem_total.unwrap_or(f64::NAN));
                            netin.push(data.net_in.unwrap_or(f64::NAN));
                            netout.push(data.net_out.unwrap_or(f64::NAN));
                            diskread.push(data.disk_read.unwrap_or(f64::NAN));
                            diskwrite.push(data.disk_write.unwrap_or(f64::NAN));
                            time.push(data.time as i64);
                        }

                        self.cpu = Rc::new(Series::new(tr!("CPU usage"), cpu));
                        self.memory = Rc::new(Series::new(tr!("RAM usage"), memory));
                        self.memory_max = Rc::new(Series::new(tr!("Total"), memory_max));
                        // TRANSLATORS: As in amount of incoming network traffic
                        self.netin = Rc::new(Series::new(tr!("Net In"), netin));
                        // TRANSLATORS: As in amount of outgoing network traffic
                        self.netout = Rc::new(Series::new(tr!("Net Out"), netout));
                        self.diskread = Rc::new(Series::new(tr!("Disk Read"), diskread));
                        self.diskwrite = Rc::new(Series::new(tr!("Disk Write"), diskwrite));
                        self.time = Rc::new(time);
                    }
                    Err(err) => self.last_rrd_error = Some(err),
                }
                self._status_timeout = Some(Timeout::new(props.rrd_interval, move || {
                    link.send_message(Msg::ReloadRrd)
                }));
                true
            }
            Msg::UpdateRrdTimeframe(rrd_time_frame) => {
                self.rrd_time_frame = rrd_time_frame;
                ctx.link().send_message(Msg::ReloadRrd);
                false
            }
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        if props.remote != old_props.remote || props.info != old_props.info {
            self.status = None;
            self.last_status_error = None;
            self.last_rrd_error = None;

            self.time = Rc::new(Vec::new());
            self.cpu = Rc::new(Series::new("", Vec::new()));
            self.memory = Rc::new(Series::new("", Vec::new()));
            self.memory_max = Rc::new(Series::new("", Vec::new()));
            self.netin = Rc::new(Series::new("", Vec::new()));
            self.netout = Rc::new(Series::new("", Vec::new()));
            self.diskread = Rc::new(Series::new("", Vec::new()));
            self.diskwrite = Rc::new(Series::new("", Vec::new()));
            self._async_pool = AsyncPool::new();
            ctx.link()
                .send_message_batch(vec![Msg::ReloadStatus, Msg::ReloadRrd]);
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
            .with_child(Fa::new("desktop"))
            .with_child(tr! {"VM '{0}'", render_qemu_name(&props.info, true)})
            .into();

        let mut status_comp = Column::new().gap(2).padding(4);

        let status = match &self.status {
            Some(status) => status,
            None => &QemuStatus {
                agent: None,
                clipboard: None,
                cpu: Some(props.info.cpu),
                cpus: Some(props.info.maxcpu),
                diskread: None,
                diskwrite: None,
                ha: json!({ "managed": 0 }),
                lock: None,
                maxdisk: Some(props.info.maxdisk as i64),
                maxmem: Some(props.info.maxmem as i64),
                mem: Some(props.info.mem as i64),
                memhost: None,
                name: None,
                netin: None,
                netout: None,
                pid: None,
                pressurecpusome: None,
                pressurecpufull: None,
                pressurememoryfull: None,
                pressurememorysome: None,
                pressureiosome: None,
                pressureiofull: None,
                qmpstatus: None,
                running_machine: None,
                running_qemu: None,
                serial: None,
                spice: None,
                status: serde_json::from_value(serde_json::Value::String(
                    props.info.status.clone(),
                ))
                .unwrap_or(IsRunning::Stopped),
                tags: None,
                template: Some(props.info.template),
                uptime: None,
                vmid: props.info.vmid,
            },
        };

        if !status.template.unwrap_or_default() {
            status_comp.add_child(make_row(
                tr!("Status"),
                Fa::new("info"),
                status
                    .qmpstatus
                    .clone()
                    .unwrap_or(status.status.to_string()),
                None,
            ));
        }

        let ha_text = if status.ha["managed"].as_i64().unwrap_or_default() > 0 {
            let ha_group = status
                .ha
                .get("group")
                .and_then(|c| c.as_str().map(|c| c.to_string()))
                .unwrap_or(tr!("none"));
            let ha_state = status
                .ha
                .get("state")
                .and_then(|c| c.as_str())
                .unwrap_or_default();
            tr!("{0}, Group: {1}", ha_state, ha_group)
        } else {
            tr!("none")
        };

        status_comp.add_child(make_row(
            tr!("HA state"),
            Fa::new("heartbeat"),
            ha_text,
            None,
        ));

        status_comp.add_child(Container::new().padding(1)); // spacer

        let cpu = status.cpu.unwrap_or_default();
        status_comp.add_child(make_row(
            tr!("CPU usage"),
            Fa::new("cpu"),
            tr!(
                "{0}% of {1} CPU(s)",
                format!("{:.2}", cpu * 100.0),
                status.cpus.unwrap_or_default()
            ),
            Some(cpu as f32),
        ));
        let mem = status.mem.unwrap_or_default() as u64;
        let maxmem = status.maxmem.unwrap_or_default() as u64;
        status_comp.add_child(crate::renderer::memory_status_row(mem, maxmem));
        status_comp.add_child(make_row(
            tr!("Bootdisk size"),
            Fa::new("database"),
            HumanByte::from(status.maxdisk.unwrap_or_default() as u64).to_string(),
            None,
        ));

        let loading = self.status.is_none() && self.last_status_error.is_none();
        Panel::new()
            .class(FlexFit)
            .title(title)
            .class(ColorScheme::Neutral)
            .with_child(
                // FIXME: add some 'visible' or 'active' property to the progress
                Progress::new()
                    .value((!loading).then_some(0.0))
                    .style("opacity", (!loading).then_some("0")),
            )
            .with_child(status_comp)
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
                            RRDGraph::new(self.time.clone())
                                .title(tr!("CPU Usage"))
                                .render_value(|v: &f64| {
                                    if v.is_finite() {
                                        format!("{:.2}%", v * 100.0)
                                    } else {
                                        v.to_string()
                                    }
                                })
                                .serie0(Some(self.cpu.clone())),
                        )
                        .with_child(
                            RRDGraph::new(self.time.clone())
                                .title(tr!("Memory usage"))
                                .render_value(|v: &f64| {
                                    if v.is_finite() {
                                        proxmox_human_byte::HumanByte::from(*v as u64).to_string()
                                    } else {
                                        v.to_string()
                                    }
                                })
                                .serie0(Some(self.memory.clone()))
                                .serie1(Some(self.memory_max.clone())),
                        )
                        .with_child(
                            RRDGraph::new(self.time.clone())
                                .title(tr!("Network Traffic"))
                                .binary(true)
                                .render_value(|v: &f64| {
                                    if v.is_finite() {
                                        proxmox_human_byte::HumanByte::from(*v as u64).to_string()
                                    } else {
                                        v.to_string()
                                    }
                                })
                                .serie0(Some(self.netin.clone()))
                                .serie1(Some(self.netout.clone())),
                        )
                        .with_child(
                            RRDGraph::new(self.time.clone())
                                .title(tr!("Disk I/O"))
                                .binary(true)
                                .render_value(|v: &f64| {
                                    if v.is_finite() {
                                        proxmox_human_byte::HumanByte::from(*v as u64).to_string()
                                    } else {
                                        v.to_string()
                                    }
                                })
                                .serie0(Some(self.diskread.clone()))
                                .serie1(Some(self.diskwrite.clone())),
                        ),
                ),
            )
            .into()
    }
}

fn make_row(title: String, icon: Fa, text: String, meter_value: Option<f32>) -> Column {
    crate::renderer::status_row(title, icon, text, meter_value, false)
}
