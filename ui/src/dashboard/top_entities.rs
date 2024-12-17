use std::rc::Rc;

use anyhow::Error;
use web_sys::HtmlElement;
use yew::{
    virtual_dom::{VComp, VNode},
    Component, NodeRef, PointerEvent, Properties, TargetCast,
};

use proxmox_yew_comp::utils::render_epoch;
use pwt::{
    css::{AlignItems, Display, JustifyContent},
    dom::align::{align_to, AlignOptions},
    props::{
        ContainerBuilder, CssBorderBuilder, CssLength, CssPaddingBuilder, EventSubscriber,
        WidgetBuilder, WidgetStyleBuilder,
    },
    tr,
    widget::{ActionIcon, Column, Container, Fa, Panel, Row},
    AsyncPool,
};

use pdm_client::types::{Resource, ResourceRrdData};

use crate::{
    get_deep_url, navigate_to,
    renderer::{render_resource_icon, render_resource_name},
};

#[derive(Properties, PartialEq)]
pub struct TopEntities {}

impl TopEntities {
    pub fn new() -> Self {
        Self {}
    }
}

impl From<TopEntities> for VNode {
    fn from(val: TopEntities) -> Self {
        let comp = VComp::new::<TopEntitiesComp>(Rc::new(val), None);
        VNode::from(comp)
    }
}

pub enum Msg {
    LoadResult(Result<Vec<(String, Resource, ResourceRrdData)>, Error>),
    ShowTooltip(PointerEvent, Resource),
    HideTooltip,
}

struct TooltipInfo {
    pos: i32,
    id: String,
    value: Option<f64>,
    time: i64,
}

struct TopEntitiesComp {
    data: Vec<(String, Resource, ResourceRrdData)>,
    _async_pool: AsyncPool,
    last_error: Option<String>,
    tooltip_info: Option<TooltipInfo>,
    tooltip_ref: NodeRef,
    tooltip_anchor: NodeRef,
}

impl TopEntitiesComp {
    async fn load() -> Result<Vec<(String, Resource, ResourceRrdData)>, Error> {
        let res = crate::pdm_client().get_top_entities().await?;
        Ok(res)
    }
}

impl Component for TopEntitiesComp {
    type Message = Msg;
    type Properties = TopEntities;

    fn create(ctx: &yew::Context<Self>) -> Self {
        let _async_pool = AsyncPool::new();
        _async_pool.send_future(ctx.link().clone(), async move {
            Msg::LoadResult(Self::load().await)
        });
        Self {
            _async_pool,
            data: Vec::new(),
            last_error: None,
            tooltip_info: None,
            tooltip_ref: NodeRef::default(),
            tooltip_anchor: NodeRef::default(),
        }
    }

    fn update(&mut self, _ctx: &yew::Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadResult(res) => match res {
                Ok(data) => {
                    self.last_error = None;
                    self.data = data;
                }
                Err(err) => {
                    self.last_error = Some(err.to_string());
                }
            },
            Msg::ShowTooltip(pointer_event, resource) => {
                let offset = pointer_event.offset_x();
                let target: Option<HtmlElement> = pointer_event.target_dyn_into();
                if let Some(node) = target {
                    let relative_pos = offset as f64 / node.get_bounding_client_rect().width();
                    for (_remote, res, rrd) in &self.data {
                        if *res != resource {
                            continue;
                        }

                        let idx = (relative_pos * rrd.data.len() as f64)
                            .round()
                            .clamp(0.0, (rrd.data.len() - 1) as f64)
                            as usize;

                        self.tooltip_info = Some(TooltipInfo {
                            pos: offset,
                            id: resource.global_id().to_string(),
                            value: rrd.data.get(idx).copied().unwrap_or_default(),
                            time: (rrd.start + rrd.resolution * idx as u64) as i64,
                        });

                        break;
                    }
                }
            }
            Msg::HideTooltip => {
                self.tooltip_info = None;
            }
        }

        true
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let mut list = Container::new()
            .padding(4)
            .class(Display::Grid)
            .class(AlignItems::Center)
            .style("grid-template-columns", "min-content auto")
            .style("gap", "var(--pwt-spacer-3)");
        let mut tooltip = None;
        for (remote, resource, rrd) in self.data.iter().rev() {
            let tooltip_anchor = if let Some(info) = self.tooltip_info.as_ref() {
                if info.id == resource.global_id() {
                    tooltip = Some(create_tooltip(remote, resource, info));
                    Some(
                        Container::new()
                            .node_ref(self.tooltip_anchor.clone())
                            .style("position", "absolute")
                            .style("pointer-events", "none")
                            .style("left", format!("{}px", info.pos)),
                    )
                } else {
                    None
                }
            } else {
                None
            };
            list.add_child(
                Row::new()
                    .onclick({
                        let link = ctx.link().clone();
                        let remote = remote.clone();
                        let resource = resource.clone();
                        move |_| {
                            navigate_to(&link, &remote, Some(&resource));
                        }
                    })
                    .style("cursor", "pointer")
                    .gap(2)
                    .class(AlignItems::Center)
                    .with_child(render_resource_icon(resource))
                    .with_child(Container::from_tag("span").with_child(remote))
                    .with_child(Container::from_tag("span").with_child("-"))
                    .with_child(render_resource_name(resource, false))
                    .with_flex_spacer()
                    // FIXME: add link
                    .with_child(ActionIcon::new("fa fa-external-link").on_activate({
                        let link = ctx.link().clone();
                        let remote = remote.clone();
                        let id = resource.id();
                        move |_| {
                            if let Some(url) = get_deep_url(&link, &remote, &id) {
                                let _ = web_sys::window().unwrap().open_with_url(&url.href());
                            }
                        }
                    }))
                    .style("white-space", "nowrap"),
            );

            list.add_child(
                graph_from_data(&rrd.data)
                    .style("flex", "5 0")
                    .onpointermove(ctx.link().callback({
                        let resource = resource.clone();
                        move |event| Msg::ShowTooltip(event, resource.clone())
                    }))
                    .onpointerleave(ctx.link().callback(|_| Msg::HideTooltip))
                    .style("position", "relative")
                    .with_optional_child(tooltip_anchor),
            );
        }
        let title: yew::Html = Row::new()
            .class(AlignItems::Center)
            .gap(2)
            .with_child(Fa::new("desktop"))
            .with_child(tr!("Guests with most CPU usage"))
            .into();
        Panel::new()
            .flex(1.0)
            .width(500)
            .min_width(400)
            .border(true)
            .title(title)
            .with_child(list)
            .with_optional_child(tooltip.map(|tooltip| {
                Container::new()
                    .node_ref(self.tooltip_ref.clone())
                    .attribute("role", "tooltip")
                    .attribute("aria-live", "polite")
                    .attribute("data-show", "")
                    .class("pwt-tooltip")
                    .class("pwt-tooltip-rich")
                    .with_child(tooltip)
            }))
            .into()
    }

    fn rendered(&mut self, _ctx: &yew::Context<Self>, _first_render: bool) {
        if let Some(tooltip_node) = self.tooltip_ref.get() {
            if let Some(anchor) = self.tooltip_anchor.get() {
                let _ = align_to(
                    anchor,
                    tooltip_node,
                    Some(
                        AlignOptions::new(
                            pwt::dom::align::Point::BottomStart,
                            pwt::dom::align::Point::TopStart,
                            pwt::dom::align::GrowDirection::None,
                        )
                        .offset(20.0, 20.0),
                    ),
                );
            }
        }
    }
}

fn create_tooltip(remote: &str, resource: &Resource, info: &TooltipInfo) -> Column {
    Column::new()
        .min_width(200)
        .gap(2)
        .with_child(Container::from_tag("h6").with_child(tr! {
            "{0} on Remote '{1}'",
            render_resource_name(resource, false),
            remote
        }))
        .with_child(
            Row::new()
                .class(JustifyContent::SpaceBetween)
                .gap(2)
                .with_child(Container::from_tag("span").with_child(tr!("CPU Usage")))
                .with_optional_child(info.value.map(|value| {
                    Container::from_tag("span").with_child(format!("{:.2}%", value * 100.0))
                }))
                .with_optional_child(
                    info.value
                        .is_none()
                        .then_some(Container::from_tag("span").with_child(tr!("No data"))),
                ),
        )
        .with_child(
            Row::new()
                .class(JustifyContent::SpaceBetween)
                .gap(2)
                .with_child(Container::from_tag("span").with_child(tr!("Time")))
                .with_child(render_epoch(info.time)),
        )
}

const WARN_CUTOFF: f64 = 0.5;

const GOOD_COLOR: &str = "var(--pwt-color-success)";
const WARN_COLOR: &str = "var(--pwt-color-warning)";
const ERR_COLOR: &str = "var(--pwt-color-error)";
const BACKGROUND_COLOR: &str = "var(--pwt-color-surface)";

const COLOR_SPACE: &str = "oklab";

fn graph_from_data(data: &Vec<Option<f64>>) -> Container {
    let mut list = Vec::new();
    for (i, point) in data.iter().enumerate() {
        if let Some(point) = point {
            let (left, left_color, right, right_color, percent) = if *point < WARN_CUTOFF {
                let point = (point / WARN_CUTOFF).clamp(0.0, 1.0);

                (
                    point * 100.0,
                    WARN_COLOR,
                    (1.0 - point) * 100.0,
                    GOOD_COLOR,
                    (i as f64) * 100.0 / data.len() as f64,
                )
            } else {
                let point = ((point - WARN_CUTOFF) / (1.0 - WARN_CUTOFF)).clamp(0.0, 1.0);
                (
                    point * 100.0,
                    ERR_COLOR,
                    (1.0 - point) * 100.0,
                    WARN_COLOR,
                    (i as f64) * 100.0 / data.len() as f64,
                )
            };
            list.push(format!(
                "color-mix(in {}, {} {:.0}%, {} {:.0}%) {:.0}%",
                COLOR_SPACE, left_color, left, right_color, right, percent
            ));
        } else if (i + 1) < data.len() {
            // only add empty point if it's not the last, since the rrd data sometimes adds
            // data points that were not collected yet.
            list.push(format!(
                "{} {:.0}% {:.0}%",
                BACKGROUND_COLOR,
                (i as f64 - 1.0) * 100.0 / data.len() as f64,
                (i as f64 + 1.0) * 100.0 / data.len() as f64,
            ));
        };
    }
    Container::new()
        .width(CssLength::Fraction(1.0))
        .class("line-usage-graph")
        .style(
            "background-image",
            format!(
                "linear-gradient(in {} to right, {})",
                COLOR_SPACE,
                list.join(", ")
            ),
        )
}
