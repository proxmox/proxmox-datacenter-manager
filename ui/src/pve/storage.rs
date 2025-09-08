use core::f64;
use std::rc::Rc;

use gloo_timers::callback::Timeout;
use yew::{
    virtual_dom::{VComp, VNode},
    Properties,
};

use proxmox_human_byte::HumanByte;
use proxmox_yew_comp::{RRDGraph, RRDTimeframe, RRDTimeframeSelector, Series, Status};
use pwt::{
    css::{AlignItems, ColorScheme, FlexFit, JustifyContent},
    prelude::*,
    props::WidgetBuilder,
    widget::{Column, Container, Fa, Panel, Progress, Row},
    AsyncPool,
};

use pdm_api_types::{resource::PveStorageResource, rrddata::PveStorageDataPoint};
use pdm_client::types::PveStorageStatus;

use crate::{
    pve::utils::{render_content_type, render_storage_type},
    renderer::separator,
};

#[derive(Clone, Debug, Properties)]
pub struct StoragePanel {
    remote: String,
    node: String,
    info: PveStorageResource,

    #[prop_or(60_000)]
    /// The interval for refreshing the rrd data
    pub rrd_interval: u32,

    #[prop_or(10_000)]
    /// The interval for refreshing the status data
    pub status_interval: u32,
}

impl PartialEq for StoragePanel {
    fn eq(&self, other: &Self) -> bool {
        if self.remote == other.remote && self.node == other.node {
            // only check some fields, so we don't update when e.g. only the cpu changes
            self.info.storage == other.info.storage
                && self.info.id == other.info.id
                && self.info.node == other.node
        } else {
            false
        }
    }
}
impl Eq for StoragePanel {}

impl StoragePanel {
    pub fn new(remote: String, node: String, info: PveStorageResource) -> Self {
        yew::props!(Self { remote, node, info })
    }
}

impl Into<VNode> for StoragePanel {
    fn into(self) -> VNode {
        VComp::new::<StoragePanelComp>(Rc::new(self), None).into()
    }
}

pub enum Msg {
    ReloadStatus,
    ReloadRrd,
    StatusResult(Result<PveStorageStatus, proxmox_client::Error>),
    RrdResult(Result<Vec<PveStorageDataPoint>, proxmox_client::Error>),
    UpdateRrdTimeframe(RRDTimeframe),
}

pub struct StoragePanelComp {
    status: Option<PveStorageStatus>,
    last_status_error: Option<proxmox_client::Error>,
    last_rrd_error: Option<proxmox_client::Error>,
    _status_timeout: Option<Timeout>,
    _rrd_timeout: Option<Timeout>,
    _async_pool: AsyncPool,

    rrd_time_frame: RRDTimeframe,

    time: Rc<Vec<i64>>,
    disk: Rc<Series>,
    disk_max: Rc<Series>,
}

impl StoragePanelComp {
    async fn reload_status(
        remote: &str,
        node: &str,
        id: &str,
    ) -> Result<PveStorageStatus, proxmox_client::Error> {
        let status = crate::pdm_client()
            .pve_storage_status(remote, node, id)
            .await?;
        Ok(status)
    }

    async fn reload_rrd(
        remote: &str,
        node: &str,
        id: &str,
        rrd_time_frame: RRDTimeframe,
    ) -> Result<Vec<PveStorageDataPoint>, proxmox_client::Error> {
        let rrd = crate::pdm_client()
            .pve_storage_rrddata(
                remote,
                node,
                id,
                rrd_time_frame.mode,
                rrd_time_frame.timeframe,
            )
            .await?;
        Ok(rrd)
    }
}

impl yew::Component for StoragePanelComp {
    type Message = Msg;

    type Properties = StoragePanel;

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
            disk: Rc::new(Series::new("", Vec::new())),
            disk_max: Rc::new(Series::new("", Vec::new())),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let link = ctx.link().clone();
        let props = ctx.props();
        let remote = props.remote.clone();
        let node = props.node.clone();
        let id = props.info.storage.clone();
        match msg {
            Msg::ReloadStatus => {
                self._async_pool.send_future(link, async move {
                    Msg::StatusResult(Self::reload_status(&remote, &node, &id).await)
                });
                false
            }
            Msg::ReloadRrd => {
                let timeframe = self.rrd_time_frame;
                self._async_pool.send_future(link, async move {
                    Msg::RrdResult(Self::reload_rrd(&remote, &node, &id, timeframe).await)
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

                        let mut disk = Vec::new();
                        let mut disk_max = Vec::new();
                        let mut time = Vec::new();
                        for data in rrd {
                            disk.push(data.disk_used.unwrap_or(f64::NAN));
                            disk_max.push(data.disk_total.unwrap_or(f64::NAN));
                            time.push(data.time as i64);
                        }

                        self.disk = Rc::new(Series::new(tr!("Usage"), disk));
                        self.disk_max = Rc::new(Series::new(tr!("Total"), disk_max));
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
            self.disk = Rc::new(Series::new("", Vec::new()));
            self.disk_max = Rc::new(Series::new("", Vec::new()));
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
            .with_child(Fa::new("database"))
            .with_child(tr! {"Storage '{0}'", props.info.storage})
            .into();

        let mut status_comp = Column::new().gap(2).padding(4);
        let status = match &self.status {
            Some(status) => status,
            None => &PveStorageStatus {
                active: None,
                avail: Some(props.info.maxdisk as i64 - props.info.disk as i64),
                content: vec![],
                enabled: None,
                shared: None,
                total: Some(props.info.maxdisk as i64),
                ty: String::new(),
                used: Some(props.info.disk as i64),
            },
        };

        status_comp = status_comp
            .with_child(make_row(
                tr!("Enabled"),
                Fa::new(if status.enabled.unwrap_or_default() {
                    "toggle-on"
                } else {
                    "toggle-off"
                }),
                String::new(),
            ))
            .with_child(make_row(
                tr!("Active"),
                Fa::from(if status.active.unwrap_or_default() {
                    Status::Success
                } else {
                    Status::Error
                }),
                String::new(),
            ))
            .with_child(make_row(
                tr!("Content"),
                Fa::new("list"),
                status
                    .content
                    .iter()
                    .map(|c| render_content_type(&c))
                    .collect::<Vec<_>>()
                    .join(", "),
            ))
            .with_child(make_row(
                tr!("Type"),
                Fa::new("database"),
                render_storage_type(&status.ty),
            ));

        status_comp.add_child(Container::new().padding(1)); // spacer

        let disk = status.used.unwrap_or_default();
        let maxdisk = status.total.unwrap_or_default();
        let disk_usage = disk as f64 / maxdisk as f64;
        status_comp.add_child(crate::renderer::status_row(
            tr!("Usage"),
            Fa::new("database"),
            tr!(
                "{0}% ({1} of {2})",
                format!("{:.2}", disk_usage * 100.0),
                HumanByte::from(disk as u64),
                HumanByte::from(maxdisk as u64),
            ),
            Some(disk_usage as f32),
            false,
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
                    Column::new().padding(4).gap(4).with_child(
                        RRDGraph::new(self.time.clone())
                            .title(tr!("Usage"))
                            .render_value(|v: &f64| {
                                if v.is_finite() {
                                    proxmox_human_byte::HumanByte::from(*v as u64).to_string()
                                } else {
                                    v.to_string()
                                }
                            })
                            .serie0(Some(self.disk.clone()))
                            .serie1(Some(self.disk_max.clone())),
                    ),
                ),
            )
            .into()
    }
}

fn make_row(title: String, icon: Fa, text: String) -> Column {
    crate::renderer::status_row(title, icon, text, None, true)
}
