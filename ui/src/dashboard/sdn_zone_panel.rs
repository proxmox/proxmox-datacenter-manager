use std::rc::Rc;

use anyhow::Error;

use pdm_api_types::resource::{ResourceType, ResourcesStatus, SdnStatus, SdnZoneCount};
use pdm_search::{Search, SearchTerm};
use pwt::{
    css::{self, FontColor, TextAlign},
    prelude::*,
    state::SharedState,
    widget::{Container, Fa, List, ListTile, Panel},
};
use yew::{
    virtual_dom::{VComp, VNode},
    Properties,
};

use crate::dashboard::create_title_with_icon;
use crate::dashboard::view::add_current_view_to_search;
use crate::search_provider::get_search_provider;
use crate::LoadResult;

use super::loading_column;

#[derive(PartialEq, Clone, Properties)]
pub struct SdnZonePanel {
    status: Option<SdnZoneCount>,
}

impl SdnZonePanel {
    pub fn new(status: Option<SdnZoneCount>) -> Self {
        yew::props!(Self { status })
    }
}

impl From<SdnZonePanel> for VNode {
    fn from(value: SdnZonePanel) -> Self {
        let comp = VComp::new::<SdnZonePanelComponent>(Rc::new(value), None);
        VNode::from(comp)
    }
}

#[derive(PartialEq, Clone)]
pub enum StatusRow {
    State(SdnStatus, u64),
    All(u64),
}

impl StatusRow {
    fn icon(&self) -> Fa {
        let (icon, color) = match self {
            Self::All(_) => ("th", None),
            Self::State(SdnStatus::Available, _) => ("check", Some(FontColor::Success)),
            Self::State(SdnStatus::Error, _) => ("times-circle", Some(FontColor::Error)),
            Self::State(SdnStatus::Unknown, _) => ("question", None),
        };

        let mut icon = Fa::new(icon);

        if let Some(color) = color {
            icon = icon.class(color);
        }

        icon
    }
}

pub struct SdnZonePanelComponent {}

impl yew::Component for SdnZonePanelComponent {
    type Message = Search;
    type Properties = SdnZonePanel;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self {}
    }

    fn update(&mut self, ctx: &Context<Self>, mut msg: Self::Message) -> bool {
        if let Some(provider) = get_search_provider(ctx) {
            add_current_view_to_search(ctx, &mut msg);
            provider.search(msg);
        }

        false
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();

        let Some(status) = &props.status else {
            return loading_column().into();
        };

        let data = vec![
            StatusRow::State(SdnStatus::Available, status.available),
            StatusRow::State(SdnStatus::Error, status.error),
            StatusRow::State(SdnStatus::Unknown, status.unknown),
            StatusRow::All(status.available + status.error + status.unknown),
        ];

        let tiles: Vec<_> = data
            .into_iter()
            .filter_map(|row| create_list_tile(ctx.link(), row))
            .collect();

        let list = List::new(tiles.len() as u64, move |idx: u64| {
            tiles[idx as usize].clone()
        })
        .padding(4)
        .class(css::Flex::Fill)
        .grid_template_columns("auto auto 1fr auto");

        list.into()
    }
}

fn create_list_tile(
    link: &html::Scope<SdnZonePanelComponent>,
    status_row: StatusRow,
) -> Option<ListTile> {
    let (icon, status, count) = match status_row {
        StatusRow::State(SdnStatus::Unknown, 0) => return None,
        StatusRow::State(sdn_status, count) => (status_row.icon(), Some(sdn_status), count),
        StatusRow::All(count) => (status_row.icon(), None, count),
    };

    let name = status
        .map(|status| status.to_string())
        .unwrap_or_else(|| "All".to_string());

    Some(
        ListTile::new()
            .tabindex(0)
            .interactive(true)
            .with_child(icon)
            .with_child(Container::new().padding_x(2).with_child(name))
            .with_child(
                Container::new()
                    .class(TextAlign::Right)
                    .padding_end(2)
                    .with_child(count),
            )
            .with_child(Fa::new("search"))
            .onclick(link.callback(move |_| create_sdn_zone_search_term(status)))
            .onkeydown(link.batch_callback(
                move |event: KeyboardEvent| match event.key().as_str() {
                    "Enter" | " " => Some(create_sdn_zone_search_term(status)),
                    _ => None,
                },
            )),
    )
}

fn create_sdn_zone_search_term(status: Option<SdnStatus>) -> Search {
    let resource_type: ResourceType = ResourceType::PveNetwork;

    let mut terms = vec![
        SearchTerm::new(resource_type.as_str()).category(Some("type")),
        SearchTerm::new("zone").category(Some("network-type")),
    ];

    if let Some(status) = status {
        terms.push(SearchTerm::new(status.to_string()).category(Some("status")));
    }

    Search::with_terms(terms)
}

pub fn create_sdn_panel(status: SharedState<LoadResult<ResourcesStatus, Error>>) -> Panel {
    let sdn_zones_status = status
        .read()
        .data
        .as_ref()
        .map(|status| status.sdn_zones.clone());

    Panel::new()
        .title(create_title_with_icon("sdn", tr!("SDN Zones")))
        .with_child(SdnZonePanel::new(sdn_zones_status))
}
