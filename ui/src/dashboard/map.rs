use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::Hash;
use std::rc::Rc;

use anyhow::Error;
use geojson::GeoJson;
use yew::virtual_dom::{VComp, VNode};

use proxmox_yew_comp::Status;
use pwt::css;
use pwt::prelude::*;
use pwt::state::{Loader, SharedState, SharedStateObserver};
use pwt::widget::canvas::Group;
use pwt::widget::charts::{
    render_point_default, render_tooltip_default, Location, MapPointData, PointsRenderArgs,
    WorldMap, WorldPoint,
};
use pwt::widget::container::span;
use pwt::widget::{error_message, ActionIcon, Column, Container, Fa, Panel, Row, Tooltip};
use pwt_macros::{builder, widget};

use crate::dashboard::loading_column;
use crate::{navigate_to, LoadResult};

use pdm_api_types::remotes::RemoteType;
use pdm_api_types::resource::{RemoteInfo, RemoteStatus, ResourcesStatus};
use pdm_api_types::CachedLocationInfo;
use pdm_api_types::Location as RemoteLocation;

#[widget(comp=DashboardMapComp, @element)]
#[builder]
#[derive(Properties, PartialEq, Clone)]
pub struct DashboardMap {
    status: SharedState<LoadResult<ResourcesStatus, Error>>,
    locations: SharedState<LoadResult<HashMap<String, CachedLocationInfo>, Error>>,
}

impl DashboardMap {
    pub fn new(
        status: SharedState<LoadResult<ResourcesStatus, Error>>,
        locations: SharedState<LoadResult<HashMap<String, CachedLocationInfo>, Error>>,
    ) -> Self {
        yew::props!(Self { status, locations })
    }
}

pub enum Msg {
    MapLoaded,
    DataChanged,
}

pub struct DashboardMapComp {
    loader: Loader<GeoJson>,
    points: Vec<WorldPoint<PoiInfo>>,
    _status_observer: SharedStateObserver<LoadResult<ResourcesStatus, Error>>,
    _location_observer: SharedStateObserver<LoadResult<HashMap<String, CachedLocationInfo>, Error>>,
}

#[derive(PartialEq, Debug)]
struct UniqueRemoteLocation(RemoteLocation, String);

// lat/long can't be NaN since the config format limits to valid values, so this is ok
impl Eq for UniqueRemoteLocation {}

impl Hash for UniqueRemoteLocation {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.name.hash(state);
        self.1.hash(state);
        self.0.latitude.to_bits().hash(state);
        self.0.longitude.to_bits().hash(state);
    }
}

impl DashboardMapComp {
    fn calculate_points(ctx: &Context<Self>) -> Vec<WorldPoint<PoiInfo>> {
        let read_guard = ctx.props().status.read();

        let mut info_map = HashMap::new();
        if let Some(data) = &read_guard.data {
            for remote in &data.remote_list {
                info_map.insert(remote.name.clone(), remote);
            }
        };

        let mut unique_locations: HashMap<UniqueRemoteLocation, Vec<String>> = HashMap::new();
        let location_guard = ctx.props().locations.read();

        if let Some(locations) = &location_guard.data {
            for (remote, remote_location) in locations {
                for (nodename, node_location) in &remote_location.node_locations {
                    let unique_location = unique_locations
                        .entry(UniqueRemoteLocation(node_location.clone(), remote.clone()))
                        .or_default();

                    unique_location.push(nodename.clone());
                }
            }
        }

        let mut points = unique_locations
            .into_iter()
            .map(|(point, members)| {
                let UniqueRemoteLocation(location, remote) = point;

                let data = match info_map.get(&remote) {
                    Some(&info) => info.clone(),
                    None => RemoteInfo {
                        name: remote,
                        ty: RemoteType::Pve,
                        messages: Vec::new(),
                        status: RemoteStatus::Unknown,
                    },
                };
                let data = PoiInfo::new(data, members, location.name);
                WorldPoint {
                    location: Location::new(location.longitude, location.latitude),
                    data,
                }
            })
            .collect::<Vec<_>>();

        points.sort_by_key(|loc| loc.data.render_title());
        points
    }
}

impl yew::Component for DashboardMapComp {
    type Message = Msg;
    type Properties = DashboardMap;

    fn create(ctx: &Context<Self>) -> Self {
        let loader = Loader::new()
            .loader((
                |url: AttrValue| async move {
                    let json = gloo_net::http::Request::get(&url).send().await?;
                    let geo_json = GeoJson::from_json_value(json.json().await?)?;
                    Ok(geo_json)
                },
                "/geojson/world-map.json",
            ))
            .on_change(ctx.link().callback(|_| Msg::MapLoaded));
        loader.load();

        let _status_observer = ctx
            .props()
            .status
            .add_listener(ctx.link().callback(|_| Msg::DataChanged));

        let _location_observer = ctx
            .props()
            .locations
            .add_listener(ctx.link().callback(|_| Msg::DataChanged));

        let points = Self::calculate_points(ctx);

        Self {
            loader,
            points,
            _status_observer,
            _location_observer,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::MapLoaded => {}
            Msg::DataChanged => {
                self.points = Self::calculate_points(ctx);
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let loader = self.loader.read();

        if !props.locations.read().has_data() {
            return loading_column().into();
        }

        let geojson = match &loader.data {
            Some(Ok(geojson)) => Rc::clone(geojson),
            Some(Err(err)) => return error_message(&err.to_string()).into(),
            _ => return loading_column().into(),
        };

        WorldMap::new(geojson)
            .with_std_props(&props.std_props)
            .listeners(&props.listeners)
            .points(self.points.clone())
            .into()
    }
}

#[derive(Clone, PartialEq, Properties)]
struct PoiInfo {
    name: Option<String>,
    remote: RemoteInfo,
    nodes: Vec<String>,
}

impl std::ops::Deref for PoiInfo {
    type Target = RemoteInfo;

    fn deref(&self) -> &Self::Target {
        &self.remote
    }
}

impl PoiInfo {
    fn new(remote: RemoteInfo, nodes: Vec<String>, name: Option<String>) -> Self {
        yew::props!(Self {
            name,
            remote,
            nodes,
        })
    }
}

impl From<PoiInfo> for VNode {
    fn from(val: PoiInfo) -> Self {
        let comp = VComp::new::<PoiInfoComp>(Rc::new(val), None);
        VNode::from(comp)
    }
}

struct PoiInfoComp {}

impl Component for PoiInfoComp {
    type Message = ();
    type Properties = PoiInfo;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {}
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();

        let link = ctx.link().clone();
        let remote_name = props.remote.name.clone();
        let (status, status_icon) = match props.remote.status {
            RemoteStatus::Good => (tr!("Good"), Fa::from(Status::Success)),
            RemoteStatus::Warning => (tr!("Warning"), Fa::from(Status::Warning)),
            RemoteStatus::Error => (tr!("Error"), Fa::from(Status::Error)),
            RemoteStatus::Unknown => (tr!("Unknown"), Fa::from(Status::Unknown)),
        };
        let mut nodes = props.nodes.clone();
        nodes.sort();
        let (node_count, node_hint) =
            match (props.remote.ty, nodes.len()) {
                (RemoteType::Pve, x) if x > 0 => (
                    Some(span(tr!("1 Node" | "{0} Nodes" % x, x))),
                    Some(Tooltip::new(Fa::new("question-circle")).rich_tip(
                        Column::new().children(nodes.into_iter().map(|n| span(n).into())),
                    )),
                ),
                _ => (None, None),
            };

        let extra_row = match (node_count, props.name.as_ref()) {
            (None, None) => None,
            (node_count, name) => Some(
                Row::new()
                    .padding_start(1)
                    .padding_end(4)
                    .gap(1)
                    .class(css::FontStyle::BodySmall)
                    .class(css::AlignItems::Center)
                    .with_optional_child(name.map(span))
                    .with_flex_spacer()
                    .with_optional_child(node_count)
                    .with_optional_child(node_hint),
            ),
        };
        Column::new()
            .width(300)
            .max_height(300)
            .class(css::JustifyContent::Stretch)
            .padding(1)
            .with_child(
                Row::new()
                    .gap(1)
                    .class(css::AlignItems::Center)
                    .with_child(status_icon)
                    .with_child(span(&status))
                    .with_flex_spacer()
                    .with_child(span(&props.remote.name))
                    .with_child(
                        ActionIcon::new("fa fa-chevron-right")
                            .on_activate(move |_| navigate_to(&link, &remote_name, None)),
                    ),
            )
            .with_optional_child(extra_row)
            .with_optional_child(
                (!props.remote.messages.is_empty()).then_some(
                    Column::new()
                        .padding_top(2)
                        .children(props.remote.messages.iter().map(|err| {
                            span(err)
                                .padding_bottom(1)
                                .class(css::Overflow::Auto)
                                .into()
                        })),
                ),
            )
            .into()
    }
}

impl MapPointData for PoiInfo {
    fn render_title(&self) -> AttrValue {
        match &self.name {
            Some(name) => format!("{} - {name}", self.remote.name).into(),
            None => self.remote.name.clone().into(),
        }
    }

    fn render_point(args: &PointsRenderArgs<Self>) -> Group {
        let mut worst = RemoteStatus::Good;

        for poi in args.points {
            match (&poi.data.status, &worst) {
                (RemoteStatus::Error, _) => worst = RemoteStatus::Error,
                (RemoteStatus::Warning, RemoteStatus::Good | RemoteStatus::Unknown) => {
                    worst = RemoteStatus::Warning
                }
                (RemoteStatus::Unknown, RemoteStatus::Good) => worst = RemoteStatus::Unknown,
                _ => {}
            }
        }

        let mut args = args.clone();
        let txt = match worst {
            RemoteStatus::Good => "success",
            RemoteStatus::Warning => "warning",
            RemoteStatus::Error => "error",
            RemoteStatus::Unknown => {
                // animate the not yet loaded remotes
                args.selected = true;
                "primary"
            }
        };
        render_point_default(&args).style("--pwt-location-color", format!("var(--pwt-color-{txt})"))
    }

    fn render_info(args: &PointsRenderArgs<Self>) -> Html {
        let mut points = args.points.to_vec();
        points.sort_by(|a, b| a.data.render_title().cmp(&b.data.render_title()));
        Column::new()
            .children(
                points
                    .iter()
                    // insert a separator in between
                    .flat_map(|&point| {
                        [Container::from_tag("hr").into(), point.data.clone().into()]
                    })
                    .skip(1),
            )
            .into()
    }

    fn render_tooltip(args: &PointsRenderArgs<Self>) -> Html {
        let mut seen = HashSet::new();
        let mut unique_remotes = args
            .points
            .iter()
            .cloned()
            .filter(|point| {
                let title = point.data.render_title();
                seen.insert(title)
            })
            .collect::<Vec<_>>();
        unique_remotes.sort_by(|a, b| a.data.render_title().cmp(&b.data.render_title()));

        let mut new_args = args.clone();
        new_args.points = &unique_remotes;

        render_tooltip_default(&new_args)
    }
}

/// Creates a dashboard panel with a world map
pub fn create_map_panel(
    status: SharedState<LoadResult<ResourcesStatus, Error>>,
    locations: SharedState<LoadResult<HashMap<String, CachedLocationInfo>, Error>>,
) -> Panel {
    Panel::new()
        .with_child(DashboardMap::new(status.clone(), locations.clone()).flex(1.0))
        .with_optional_child(
            status
                .read()
                .error
                .as_ref()
                .map(|err| error_message(&format!("status - {err}"))),
        )
        .with_optional_child(
            locations
                .read()
                .error
                .as_ref()
                .map(|err| error_message(&format!("locations - {err}"))),
        )
}
