use std::rc::Rc;

use gloo_timers::callback::Timeout;
use pbs_api_types::DataStoreConfig;
use yew::{
    virtual_dom::{VComp, VNode},
    Properties,
};

use proxmox_yew_comp::{RRDGraph, RRDTimeframe, RRDTimeframeSelector, Series, StatusRow};
use pwt::{
    css::{ColorScheme, FlexFit, JustifyContent},
    prelude::*,
    props::WidgetBuilder,
    widget::{Column, Container, Progress, Row},
    AsyncPool,
};

use pdm_api_types::rrddata::PbsDatastoreDataPoint;

use crate::renderer::separator;

#[derive(Clone, PartialEq, Properties)]
pub struct DataStoreOverview {
    remote: String,
    config: DataStoreConfig,

    #[prop_or(60_000)]
    /// The interval for refreshing the rrd data
    pub rrd_interval: u32,

    #[prop_or(10_000)]
    /// The interval for refreshing the status data
    pub status_interval: u32,
}

impl Eq for DataStoreOverview {}

impl DataStoreOverview {
    pub fn new(remote: String, config: DataStoreConfig) -> Self {
        yew::props!(Self { remote, config })
    }
}

impl From<DataStoreOverview> for VNode {
    fn from(val: DataStoreOverview) -> Self {
        VComp::new::<DataStoreOverviewComp>(Rc::new(val), None).into()
    }
}

pub enum Msg {
    ReloadRrd,
    RrdResult(Result<Vec<PbsDatastoreDataPoint>, proxmox_client::Error>),
    UpdateRrdTimeframe(RRDTimeframe),
}

pub struct DataStoreOverviewComp {
    loaded: bool,
    last_rrd_error: Option<proxmox_client::Error>,
    _status_timeout: Option<Timeout>,
    _async_pool: AsyncPool,

    rrd_time_frame: RRDTimeframe,

    time: Rc<Vec<i64>>,
    disk: Rc<Series>,
    disk_max: Rc<Series>,
    disk_read: Rc<Series>,
    disk_write: Rc<Series>,
}

impl DataStoreOverviewComp {
    async fn reload_rrd(
        remote: &str,
        id: &str,
        rrd_time_frame: RRDTimeframe,
    ) -> Result<Vec<PbsDatastoreDataPoint>, proxmox_client::Error> {
        let rrd = crate::pdm_client()
            .pbs_datastore_rrddata(remote, id, rrd_time_frame.mode, rrd_time_frame.timeframe)
            .await?;
        Ok(rrd)
    }
}

impl yew::Component for DataStoreOverviewComp {
    type Message = Msg;

    type Properties = DataStoreOverview;

    fn create(ctx: &yew::Context<Self>) -> Self {
        ctx.link().send_message(Msg::ReloadRrd);
        Self {
            loaded: false,
            _async_pool: AsyncPool::new(),
            _status_timeout: None,
            last_rrd_error: None,

            rrd_time_frame: RRDTimeframe::load(),

            time: Rc::new(Vec::new()),
            disk: Rc::new(Series::new("", Vec::new())),
            disk_max: Rc::new(Series::new("", Vec::new())),
            disk_read: Rc::new(Series::new("", Vec::new())),
            disk_write: Rc::new(Series::new("", Vec::new())),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let link = ctx.link().clone();
        let props = ctx.props();
        let remote = props.remote.clone();
        let id = props.config.name.clone();
        match msg {
            Msg::ReloadRrd => {
                let timeframe = self.rrd_time_frame;
                self._async_pool.send_future(link, async move {
                    Msg::RrdResult(Self::reload_rrd(&remote, &id, timeframe).await)
                });
                false
            }
            Msg::RrdResult(res) => {
                match res {
                    Ok(rrd) => {
                        self.last_rrd_error = None;

                        let mut disk = Vec::new();
                        let mut disk_max = Vec::new();
                        let mut disk_read = Vec::new();
                        let mut disk_write = Vec::new();
                        let mut time = Vec::new();
                        for data in rrd {
                            disk.push(data.disk_used.unwrap_or(f64::NAN));
                            disk_max.push(data.disk_total.unwrap_or(f64::NAN));
                            disk_read.push(data.disk_read.unwrap_or(f64::NAN));
                            disk_write.push(data.disk_write.unwrap_or(f64::NAN));
                            time.push(data.time as i64);
                        }

                        self.disk = Rc::new(Series::new(tr!("Usage"), disk));
                        self.disk_max = Rc::new(Series::new(tr!("Total"), disk_max));
                        self.disk_read = Rc::new(Series::new(tr!("Disk Read"), disk_read));
                        self.disk_write = Rc::new(Series::new(tr!("Disk Write"), disk_write));
                        self.time = Rc::new(time);
                    }
                    Err(err) => self.last_rrd_error = Some(err),
                }
                self._status_timeout = Some(Timeout::new(props.rrd_interval, move || {
                    link.send_message(Msg::ReloadRrd)
                }));
                self.loaded = true;
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

        if props.remote != old_props.remote || props.config != old_props.config {
            self.last_rrd_error = None;

            self.time = Rc::new(Vec::new());
            self.disk = Rc::new(Series::new("", Vec::new()));
            self.disk_max = Rc::new(Series::new("", Vec::new()));
            self.disk_read = Rc::new(Series::new("", Vec::new()));
            self.disk_write = Rc::new(Series::new("", Vec::new()));
            self._async_pool = AsyncPool::new();
            ctx.link().send_message(Msg::ReloadRrd);
            true
        } else {
            false
        }
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();

        // TODO get current status via API and show usage, etc.

        Container::new()
            .class(FlexFit)
            .class(ColorScheme::Neutral)
            .with_child(
                // FIXME: add some 'visible' or 'active' property to the progress
                Progress::new()
                    .value((self.loaded).then_some(0.0))
                    .style("opacity", (self.loaded).then_some("0")),
            )
            .with_child(
                Column::new()
                    .gap(2)
                    .padding(4)
                    .with_child(
                        StatusRow::new(tr!("Path"))
                            .icon_class("fa fa-fw fa-folder-o")
                            .status(&props.config.path),
                    )
                    .with_optional_child(props.config.comment.as_deref().map(|comment| {
                        StatusRow::new(tr!("Comment"))
                            .icon_class("fa fa-fw fa-comment-o")
                            .status(comment)
                    }))
                    .with_optional_child(props.config.maintenance_mode.as_deref().map(|mode| {
                        StatusRow::new(tr!("Maintenance Mode"))
                            .icon_class("fa fa-fw fa-wrench")
                            .status(mode)
                    })),
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
                        )
                        .with_child(
                            RRDGraph::new(self.time.clone())
                                .title(tr!("Disk I/O"))
                                .render_value(|v: &f64| {
                                    if v.is_finite() {
                                        proxmox_human_byte::HumanByte::from(*v as u64).to_string()
                                    } else {
                                        v.to_string()
                                    }
                                })
                                .serie0(Some(self.disk_read.clone()))
                                .serie1(Some(self.disk_write.clone())),
                        ),
                ),
            )
            .into()
    }
}
