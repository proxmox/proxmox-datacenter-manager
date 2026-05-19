use std::rc::Rc;

use anyhow::Error;
use yew::virtual_dom::{VComp, VNode};

use proxmox_node_status::NodePowerCommand;
use proxmox_time::epoch_i64;
use proxmox_yew_comp::percent_encoding::percent_encode_component;
use proxmox_yew_comp::utils::{copy_text_to_clipboard, render_epoch};
use proxmox_yew_comp::{
    http_post, rrd_value_renderer, ConfirmButton, NodeStatusPanel, RRDGraph, RRDGrid, RRDTimeframe,
    RRDTimeframeSelector, Series,
};
use pwt::css::JustifyContent;
use pwt::widget::{Button, Column, Container, Row};
use pwt::AsyncAbortGuard;
use pwt::{prelude::*, AsyncPool};

use pdm_api_types::rrddata::PdmNodeDatapoint;

use crate::{get_nodename, renderer};

#[derive(Properties, Clone, PartialEq)]
pub(crate) struct NodeStatus {
    #[prop_or(60_000)]
    /// The interval for refreshing the rrd data
    pub rrd_interval: u32,
}

impl NodeStatus {
    /// Create new [`NodeStatus`] panel.
    pub(crate) fn new() -> Self {
        yew::props!(Self {})
    }
}

impl From<NodeStatus> for VNode {
    fn from(value: NodeStatus) -> Self {
        VComp::new::<PdmNodeStatus>(Rc::new(value), None).into()
    }
}

enum Msg {
    Reload,
    ReloadRrd,
    UpdateRrdTimeframe(RRDTimeframe),
    Error(Error),
    RebootOrShutdown(NodePowerCommand),
    ShowSystemReport(bool),
    ShowPackageVersions(bool),
    RrdLoadFinished(Result<Vec<PdmNodeDatapoint>, proxmox_client::Error>),
}

struct PdmNodeStatus {
    time_data: Rc<Vec<i64>>,

    cpu_data: Rc<Series>,
    iowait_data: Rc<Series>,
    load_data: Rc<Series>,
    mem_data: Rc<Series>,
    mem_total_data: Rc<Series>,
    swap_data: Rc<Series>,
    swap_total_data: Rc<Series>,
    disk_usage_data: Rc<Series>,
    disk_total_data: Rc<Series>,
    disk_transfer_read_data: Rc<Series>,
    disk_transfer_write_data: Rc<Series>,
    disk_iops_read_data: Rc<Series>,
    disk_iops_write_data: Rc<Series>,
    cpu_pressure_some_data: Rc<Series>,
    mem_pressure_some_data: Rc<Series>,
    mem_pressure_full_data: Rc<Series>,
    io_pressure_some_data: Rc<Series>,
    io_pressure_full_data: Rc<Series>,
    net_in: Rc<Series>,
    net_out: Rc<Series>,

    rrd_time_frame: RRDTimeframe,
    error: Option<Error>,
    abort_guard: Option<AsyncAbortGuard>,
    show_system_report: bool,
    show_package_versions: bool,

    async_pool: AsyncPool,
    _timeout: Option<gloo_timers::callback::Timeout>,
}

impl PdmNodeStatus {
    async fn reload_rrd(rrd_time_frame: RRDTimeframe) -> Msg {
        let res = crate::pdm_client()
            .get_pdm_node_rrddata(rrd_time_frame.mode, rrd_time_frame.timeframe)
            .await;

        Msg::RrdLoadFinished(res)
    }

    fn change_power_state(&mut self, ctx: &yew::Context<Self>, command: NodePowerCommand) {
        let link = ctx.link().clone();
        self.abort_guard.replace(AsyncAbortGuard::spawn(async move {
            let data = Some(serde_json::json!({"command": command}));

            let res = match http_post("/nodes/localhost/status", data).await {
                Ok(()) => Msg::Reload,
                Err(e) => Msg::Error(e),
            };

            link.send_message(res);
        }));
    }

    fn create_system_report_dialog(&self, ctx: &yew::Context<Self>) -> Html {
        // copied over from subscription_panel in proxmox-yew-comp; TODO: create own component and
        // provide download functionality.
        proxmox_yew_comp::DataViewWindow::new(tr!("System Report"))
            .width(800)
            .height(640)
            .loader("/nodes/localhost/report")
            .renderer(|report: &String| {
                let report = report.to_owned();
                Column::new()
                    .class(pwt::css::FlexFit)
                    .with_child(
                        Container::from_tag("pre")
                            .class("pwt-flex-fit pwt-font-monospace")
                            .padding(2)
                            .style("line-height", "normal")
                            .with_child(&report),
                    )
                    .with_child(
                        Row::new()
                            .padding(2)
                            .gap(1)
                            .with_flex_spacer()
                            .with_child(
                                Button::new(tr!("Copy to clipboard"))
                                    .icon_class("fa fa-clipboard")
                                    .class(pwt::css::ColorScheme::Primary)
                                    .on_activate({
                                        let report = report.clone();
                                        move |_| copy_text_to_clipboard(&report)
                                    }),
                            )
                            .with_child({
                                let button = Button::new(tr!("Download"))
                                    .icon_class("fa fa-download")
                                    .class(pwt::css::ColorScheme::Primary);

                                let data = format!(
                                    "data:text/plain;charset=utf-8,{}",
                                    percent_encode_component(&report)
                                );
                                let timestamp = render_epoch(epoch_i64());

                                let filename = match get_nodename() {
                                    Some(nodename) => {
                                        format!("{nodename}-pdm-report-{timestamp}.txt")
                                    }
                                    None => format!("pdm-report-{timestamp}.txt"),
                                };

                                html! { <a href={data} download={filename}>{button}</a> }
                            }),
                    )
                    .into()
            })
            .on_done(ctx.link().callback(|_| Msg::ShowSystemReport(false)))
            .into()
    }

    fn create_package_version_dialog(&self, ctx: &yew::Context<Self>) -> Html {
        // TODO: factor out to dedicated helper/component in proxmox-yew-comp
        proxmox_yew_comp::DataViewWindow::new(tr!("Package Versions"))
            .width(600)
            .height(600)
            .loader("/nodes/localhost/apt/versions")
            .renderer(|versions: &serde_json::Value| {
                use std::fmt::Write;

                let mut text = String::new();
                if let Some(pkgs) = versions.as_array() {
                    // loosely adapted from our JS code in proxmox-widget-toolkit
                    for pkg in pkgs {
                        let old_version =
                            pkg.get("OldVersion").and_then(|v| v.as_str()).unwrap_or("");
                        let current_state = pkg
                            .get("CurrentState")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let name = pkg.get("Package").and_then(|v| v.as_str()).unwrap_or("");
                        let extra_info =
                            pkg.get("ExtraInfo").and_then(|v| v.as_str()).unwrap_or("");

                        let version = if !old_version.is_empty() && old_version != "unknown" {
                            old_version
                        } else if current_state == "ConfigFiles" {
                            "residual config"
                        } else {
                            "not correctly installed"
                        };

                        if !extra_info.is_empty() {
                            let _ = writeln!(text, "{name}: {version} ({extra_info})");
                        } else {
                            let _ = writeln!(text, "{name}: {version}");
                        }
                    }
                }
                Column::new()
                    .class(pwt::css::FlexFit)
                    .with_child(
                        Container::from_tag("pre")
                            .class("pwt-flex-fit pwt-font-monospace")
                            .padding(2)
                            .style("line-height", "normal")
                            .with_child(&text),
                    )
                    .with_child(
                        Row::new().padding(2).with_flex_spacer().with_child(
                            Button::new(tr!("Copy to clipboard"))
                                .icon_class("fa fa-clipboard")
                                .class(pwt::css::ColorScheme::Primary)
                                .on_activate(move |_| copy_text_to_clipboard(&text)),
                        ),
                    )
                    .into()
            })
            .on_done(ctx.link().callback(|_| Msg::ShowPackageVersions(false)))
            .into()
    }
}

impl Component for PdmNodeStatus {
    type Message = Msg;
    type Properties = NodeStatus;

    fn create(ctx: &yew::Context<Self>) -> Self {
        ctx.link().send_message(Msg::ReloadRrd);

        Self {
            time_data: Rc::new(Vec::new()),

            cpu_data: empty_series(),
            cpu_pressure_some_data: empty_series(),
            mem_pressure_some_data: empty_series(),
            mem_pressure_full_data: empty_series(),
            io_pressure_some_data: empty_series(),
            io_pressure_full_data: empty_series(),
            iowait_data: empty_series(),
            load_data: empty_series(),
            mem_data: empty_series(),
            mem_total_data: empty_series(),
            swap_data: empty_series(),
            swap_total_data: empty_series(),
            net_in: empty_series(),
            net_out: empty_series(),
            disk_usage_data: empty_series(),
            disk_total_data: empty_series(),
            disk_transfer_read_data: empty_series(),
            disk_transfer_write_data: empty_series(),
            disk_iops_read_data: empty_series(),
            disk_iops_write_data: empty_series(),

            async_pool: AsyncPool::new(),
            _timeout: None,

            rrd_time_frame: RRDTimeframe::load(),
            error: None,
            abort_guard: None,
            show_system_report: false,
            show_package_versions: false,
        }
    }

    fn update(&mut self, ctx: &yew::Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::RebootOrShutdown(command) => {
                self.change_power_state(ctx, command);
                false
            }
            Msg::Error(e) => {
                self.error = Some(e);
                true
            }
            Msg::Reload => true,
            Msg::ShowSystemReport(show_system_report) => {
                self.show_system_report = show_system_report;
                true
            }
            Msg::ShowPackageVersions(show_package_versions) => {
                self.show_package_versions = show_package_versions;
                true
            }
            Msg::ReloadRrd => {
                self._timeout = None;
                let timeframe = self.rrd_time_frame;
                self.async_pool.send_future(ctx.link().clone(), async move {
                    Self::reload_rrd(timeframe).await
                });
                true
            }
            Msg::RrdLoadFinished(res) => match res {
                Ok(data_points) => {
                    self.error = None;
                    let mut cpu_vec = Vec::with_capacity(data_points.len());
                    let mut cpu_pressure_some_vec = Vec::with_capacity(data_points.len());
                    let mut iowait_vec = Vec::with_capacity(data_points.len());
                    let mut load_vec = Vec::with_capacity(data_points.len());
                    let mut mem_vec = Vec::with_capacity(data_points.len());
                    let mut mem_total_vec = Vec::with_capacity(data_points.len());
                    let mut swap_vec = Vec::with_capacity(data_points.len());
                    let mut swap_total_vec = Vec::with_capacity(data_points.len());
                    let mut mem_pressure_some_vec = Vec::with_capacity(data_points.len());
                    let mut mem_pressure_full_vec = Vec::with_capacity(data_points.len());
                    let mut io_pressure_some_vec = Vec::with_capacity(data_points.len());
                    let mut io_pressure_full_vec = Vec::with_capacity(data_points.len());
                    let mut time_vec = Vec::with_capacity(data_points.len());
                    let mut net_in_vec = Vec::with_capacity(data_points.len());
                    let mut net_out_vec = Vec::with_capacity(data_points.len());
                    let mut disk_usage_vec = Vec::with_capacity(data_points.len());
                    let mut disk_total_vec = Vec::with_capacity(data_points.len());
                    let mut disk_transfer_read_vec = Vec::with_capacity(data_points.len());
                    let mut disk_transfer_write_vec = Vec::with_capacity(data_points.len());
                    let mut disk_iops_read_vec = Vec::with_capacity(data_points.len());
                    let mut disk_iops_write_vec = Vec::with_capacity(data_points.len());

                    for data in data_points {
                        cpu_vec.push(data.cpu_current.unwrap_or(f64::NAN));
                        iowait_vec.push(data.cpu_iowait.unwrap_or(f64::NAN));
                        load_vec.push(data.cpu_avg1.unwrap_or(f64::NAN));
                        cpu_pressure_some_vec
                            .push(data.cpu_pressure_some_avg10.unwrap_or(f64::NAN));
                        mem_vec.push(data.mem_used.unwrap_or(f64::NAN));
                        mem_total_vec.push(data.mem_total.unwrap_or(f64::NAN));
                        swap_vec.push(data.swap_used.unwrap_or(f64::NAN));
                        swap_total_vec.push(data.swap_total.unwrap_or(f64::NAN));
                        mem_pressure_some_vec
                            .push(data.mem_pressure_some_avg10.unwrap_or(f64::NAN));
                        mem_pressure_full_vec
                            .push(data.mem_pressure_full_avg10.unwrap_or(f64::NAN));
                        net_in_vec.push(data.net_in.unwrap_or(f64::NAN));
                        net_out_vec.push(data.net_out.unwrap_or(f64::NAN));
                        io_pressure_some_vec.push(data.io_pressure_some_avg10.unwrap_or(f64::NAN));
                        io_pressure_full_vec.push(data.io_pressure_full_avg10.unwrap_or(f64::NAN));

                        disk_total_vec.push(data.disk_total.unwrap_or(f64::NAN));
                        disk_usage_vec.push(data.disk_used.unwrap_or(f64::NAN));
                        disk_transfer_read_vec.push(data.disk_read.unwrap_or(f64::NAN));
                        disk_transfer_write_vec.push(data.disk_write.unwrap_or(f64::NAN));

                        disk_iops_read_vec.push(data.disk_read_iops.unwrap_or(f64::NAN));
                        disk_iops_write_vec.push(data.disk_write_iops.unwrap_or(f64::NAN));

                        time_vec.push(data.time as i64);
                    }

                    self.cpu_data = Rc::new(Series::new(tr!("CPU usage"), cpu_vec));
                    self.iowait_data = Rc::new(Series::new(tr!("IO delay"), iowait_vec));
                    self.load_data = Rc::new(Series::new(tr!("Server Load"), load_vec));
                    self.cpu_pressure_some_data =
                        Rc::new(Series::new(tr!("Some"), cpu_pressure_some_vec));
                    self.mem_data = Rc::new(Series::new(tr!("Used Memory"), mem_vec));
                    self.mem_total_data = Rc::new(Series::new(tr!("Total Memory"), mem_total_vec));
                    self.swap_data = Rc::new(Series::new(tr!("Used Swap"), swap_vec));
                    self.swap_total_data = Rc::new(Series::new(tr!("Total Swap"), swap_total_vec));
                    self.mem_pressure_some_data =
                        Rc::new(Series::new(tr!("Some"), mem_pressure_some_vec));
                    self.mem_pressure_full_data =
                        Rc::new(Series::new(tr!("Full"), mem_pressure_full_vec));
                    self.io_pressure_some_data =
                        Rc::new(Series::new(tr!("Some"), io_pressure_some_vec));
                    self.io_pressure_full_data =
                        Rc::new(Series::new(tr!("Full"), io_pressure_full_vec));

                    self.net_in = Rc::new(Series::new(tr!("Incoming"), net_in_vec));
                    self.net_out = Rc::new(Series::new(tr!("Outgoing"), net_out_vec));

                    self.disk_usage_data = Rc::new(Series::new(tr!("Used Disk"), disk_usage_vec));
                    self.disk_total_data = Rc::new(Series::new(tr!("Total Disk"), disk_total_vec));
                    self.disk_transfer_read_data =
                        Rc::new(Series::new(tr!("Read"), disk_transfer_read_vec));
                    self.disk_transfer_write_data =
                        Rc::new(Series::new(tr!("Write"), disk_transfer_write_vec));
                    self.disk_iops_read_data =
                        Rc::new(Series::new(tr!("Read"), disk_iops_read_vec));
                    self.disk_iops_write_data =
                        Rc::new(Series::new(tr!("Write"), disk_iops_write_vec));

                    self.time_data = Rc::new(time_vec);

                    let link = ctx.link().clone();
                    self._timeout = Some(gloo_timers::callback::Timeout::new(
                        ctx.props().rrd_interval,
                        move || link.send_message(Msg::ReloadRrd),
                    ));

                    true
                }
                Err(err) => {
                    self.error = Some(err.into());
                    true
                }
            },
            Msg::UpdateRrdTimeframe(rrd_time_frame) => {
                self.rrd_time_frame = rrd_time_frame;
                ctx.link().send_message(Msg::ReloadRrd);
                false
            }
        }
    }

    fn view(&self, ctx: &yew::Context<Self>) -> Html {
        Column::new()
            .class(pwt::css::FlexFit)
            .with_child(
                Container::new()
                    .class("pwt-content-spacer-padding")
                    .class("pwt-content-spacer-colors")
                    .class("pwt-default-colors")
                    .padding_bottom(0)
                    .with_child(
                        Row::new()
                            .gap(1)
                            .with_child(
                                Button::new(tr!("Package Versions"))
                                    .class(pwt::css::ColorScheme::Neutral)
                                    .icon_class("fa fa-gift")
                                    .onclick(
                                        ctx.link().callback(|_| Msg::ShowPackageVersions(true)),
                                    ),
                            )
                            .with_child(
                                Button::new(tr!("System Report"))
                                    .class(pwt::css::ColorScheme::Neutral)
                                    .icon_class("fa fa-stethoscope")
                                    .onclick(ctx.link().callback(|_| Msg::ShowSystemReport(true))),
                            )
                            .with_flex_spacer()
                            .with_child(
                                ConfirmButton::new(tr!("Reboot Datacenter Manager"))
                                    .confirm_message(tr!(
                                        "Are you sure you want to reboot the node?"
                                    ))
                                    .on_activate(ctx.link().callback(|_| {
                                        Msg::RebootOrShutdown(NodePowerCommand::Reboot)
                                    }))
                                    .class(pwt::css::ColorScheme::Neutral)
                                    .icon_class("fa pwt-color-warning fa-undo"),
                            )
                            .with_child(
                                ConfirmButton::new(tr!("Shutdown Datacenter Manager"))
                                    .confirm_message(tr!(
                                        "Are you sure you want to shut down the node?"
                                    ))
                                    .on_activate(ctx.link().callback(|_| {
                                        Msg::RebootOrShutdown(NodePowerCommand::Shutdown)
                                    }))
                                    .class(pwt::css::ColorScheme::Neutral)
                                    .icon_class("fa pwt-color-error fa-power-off"),
                            ),
                    ),
            )
            .with_child(
                Column::new()
                    .class("pwt-content-spacer-padding")
                    .class("pwt-content-spacer-colors")
                    .class("pwt-default-colors")
                    .class(pwt::css::FlexFit)
                    .with_child(
                        NodeStatusPanel::new()
                            .status_base_url("/nodes/localhost/status")
                            .with_child(renderer::separator().padding_x(4))
                            .with_optional_child(
                                self.error
                                    .as_ref()
                                    .map(|err| pwt::widget::error_message(&err.to_string())),
                            )
                            .with_child(
                                Row::new()
                                    .padding_x(4)
                                    .padding_y(1)
                                    .class(JustifyContent::FlexEnd)
                                    .with_child(
                                        RRDTimeframeSelector::new().on_change(
                                            ctx.link().callback(Msg::UpdateRrdTimeframe),
                                        ),
                                    ),
                            )
                            .with_child(
                                RRDGrid::new()
                                    .with_child(
                                        RRDGraph::new(self.time_data.clone())
                                            .title(tr!("CPU Usage"))
                                            .render_value(rrd_value_renderer::render_cpu_usage)
                                            .serie0(Some(self.cpu_data.clone()))
                                            .serie1(Some(self.iowait_data.clone())),
                                    )
                                    .with_child(
                                        RRDGraph::new(self.time_data.clone())
                                            .title(tr!("Server Load"))
                                            .render_value(rrd_value_renderer::render_load)
                                            .serie0(Some(self.load_data.clone())),
                                    )
                                    .with_child(
                                        RRDGraph::new(self.time_data.clone())
                                            .title(tr!("Memory Usage"))
                                            .binary(true)
                                            .render_value(rrd_value_renderer::render_bytes)
                                            .serie0(Some(self.mem_total_data.clone()))
                                            .serie1(Some(self.mem_data.clone())),
                                    )
                                    .with_child(
                                        RRDGraph::new(self.time_data.clone())
                                            .title(tr!("Swap Usage"))
                                            .binary(true)
                                            .render_value(rrd_value_renderer::render_bytes)
                                            .serie0(Some(self.swap_total_data.clone()))
                                            .serie1(Some(self.swap_data.clone())),
                                    )
                                    .with_child(
                                        RRDGraph::new(self.time_data.clone())
                                            .title(tr!("Network Traffic"))
                                            .binary(true)
                                            .render_value(rrd_value_renderer::render_bandwidth)
                                            .serie0(Some(self.net_in.clone()))
                                            .serie1(Some(self.net_out.clone())),
                                    )
                                    .with_child(
                                        RRDGraph::new(self.time_data.clone())
                                            .title(tr!("CPU Pressure Stall"))
                                            .render_value(rrd_value_renderer::render_pressure)
                                            .serie0(Some(self.cpu_pressure_some_data.clone())),
                                    )
                                    .with_child(
                                        RRDGraph::new(self.time_data.clone())
                                            .title(tr!("Memory Pressure Stall"))
                                            .render_value(rrd_value_renderer::render_pressure)
                                            .serie0(Some(self.mem_pressure_some_data.clone()))
                                            .serie1(Some(self.mem_pressure_full_data.clone())),
                                    )
                                    .with_child(
                                        RRDGraph::new(self.time_data.clone())
                                            .title(tr!("IO Pressure Stall"))
                                            .render_value(rrd_value_renderer::render_pressure)
                                            .serie0(Some(self.io_pressure_some_data.clone()))
                                            .serie1(Some(self.io_pressure_full_data.clone())),
                                    )
                                    .with_child(
                                        RRDGraph::new(self.time_data.clone())
                                            .title(tr!("Root Disk Usage"))
                                            .render_value(rrd_value_renderer::render_bytes)
                                            .serie0(Some(self.disk_usage_data.clone()))
                                            .serie1(Some(self.disk_total_data.clone())),
                                    )
                                    .with_child(
                                        RRDGraph::new(self.time_data.clone())
                                            .title(tr!("Root Disk Transfer Rate"))
                                            .binary(true)
                                            .render_value(rrd_value_renderer::render_bandwidth)
                                            .serie0(Some(self.disk_transfer_read_data.clone()))
                                            .serie1(Some(self.disk_transfer_write_data.clone())),
                                    )
                                    .with_child(
                                        RRDGraph::new(self.time_data.clone())
                                            .title(tr!("Root Disk IOPS"))
                                            .serie0(Some(self.disk_iops_read_data.clone()))
                                            .serie1(Some(self.disk_iops_write_data.clone())),
                                    ),
                            ),
                    ),
            )
            .with_optional_child(
                self.show_system_report
                    .then_some(self.create_system_report_dialog(ctx)),
            )
            .with_optional_child(
                self.show_package_versions
                    .then_some(self.create_package_version_dialog(ctx)),
            )
            .into()
    }
}

fn empty_series() -> Rc<Series> {
    Rc::new(Series::new("", Vec::new()))
}
