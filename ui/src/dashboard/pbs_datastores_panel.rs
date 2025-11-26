use std::rc::Rc;

use anyhow::Error;
use yew::virtual_dom::{VComp, VNode};

use pdm_api_types::resource::{PbsDatastoreStatusCount, ResourceType, ResourcesStatus};
use pdm_search::{Search, SearchTerm};
use proxmox_yew_comp::Status;
use pwt::css::{self, TextAlign};
use pwt::prelude::*;
use pwt::state::SharedState;
use pwt::widget::{Container, Fa, List, ListTile, Panel};

use crate::dashboard::create_title_with_icon;
use crate::dashboard::view::add_current_view_to_search;
use crate::search_provider::get_search_provider;
use crate::LoadResult;

use super::loading_column;

#[derive(PartialEq, Clone, Properties)]
pub struct PbsDatastoresPanel {
    status: Option<PbsDatastoreStatusCount>,
}

impl PbsDatastoresPanel {
    /// Create new datastore status panel with given storage status counts
    pub fn new(status: Option<PbsDatastoreStatusCount>) -> Self {
        yew::props!(Self { status })
    }
}

impl From<PbsDatastoresPanel> for VNode {
    fn from(value: PbsDatastoresPanel) -> Self {
        let comp = VComp::new::<PbsDatastoresPanelComponent>(Rc::new(value), None);
        VNode::from(comp)
    }
}

#[derive(PartialEq, Clone)]
pub enum StatusRow {
    Online(u64),
    UnderMaintenance(u64),
    HighUsage(u64),
    Unknown(u64),
    All(u64),
}

pub struct PbsDatastoresPanelComponent {}

impl yew::Component for PbsDatastoresPanelComponent {
    type Message = Search;
    type Properties = PbsDatastoresPanel;

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
            StatusRow::Online(status.online),
            StatusRow::UnderMaintenance(status.under_maintenance.unwrap_or_default()),
            StatusRow::HighUsage(status.high_usage.unwrap_or_default()),
            StatusRow::Unknown(status.unknown.unwrap_or_default()),
            StatusRow::All(status.online + status.under_maintenance.unwrap_or_default()),
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
    link: &html::Scope<PbsDatastoresPanelComponent>,
    status_row: StatusRow,
) -> Option<ListTile> {
    let (icon, count, name, search_term) = match status_row {
        StatusRow::Online(count) => (
            Fa::from(Status::Success),
            count,
            tr!("Online"),
            Some(("online", "status")),
        ),
        StatusRow::HighUsage(count) => (
            Fa::from(Status::Warning),
            count,
            tr!("High usage"),
            Some(("high-usage", "property")),
        ),
        StatusRow::UnderMaintenance(count) => (
            Fa::new("wrench"),
            count,
            tr!("Under Maintenance"),
            Some(("under-maintenance", "status")),
        ),
        StatusRow::Unknown(count) => (
            Fa::from(Status::Unknown),
            count,
            tr!("Unknown"),
            Some(("unknown", "property")),
        ),
        StatusRow::All(count) => (Fa::new("database"), count, tr!("All"), None),
    };

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
            .onclick(link.callback(move |_| create_pbs_datastores_status_search_term(search_term)))
            .onkeydown(link.batch_callback(
                move |event: KeyboardEvent| match event.key().as_str() {
                    "Enter" | " " => Some(create_pbs_datastores_status_search_term(search_term)),
                    _ => None,
                },
            )),
    )
}

fn create_pbs_datastores_status_search_term(search_term: Option<(&str, &str)>) -> Search {
    let resource_type: ResourceType = ResourceType::PbsDatastore;
    let mut terms = vec![SearchTerm::new(resource_type.as_str()).category(Some("type"))];
    if let Some((search_term, category)) = search_term {
        terms.push(SearchTerm::new(search_term).category(Some(category)));
    }
    Search::with_terms(terms)
}

pub fn create_pbs_datastores_panel(
    status: SharedState<LoadResult<ResourcesStatus, Error>>,
) -> Panel {
    let pbs_datastores = status
        .read()
        .data
        .as_ref()
        .map(|status| status.pbs_datastores.clone());

    Panel::new()
        .title(create_title_with_icon(
            "database",
            tr!("Backup Server Datastores"),
        ))
        .border(true)
        .with_child(PbsDatastoresPanel::new(pbs_datastores))
}
