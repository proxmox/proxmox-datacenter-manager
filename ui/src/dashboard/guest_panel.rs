use std::rc::Rc;

use pdm_api_types::resource::{GuestStatusCount, ResourceType};
use pdm_search::{Search, SearchTerm};
use proxmox_yew_comp::GuestState;
use pwt::{
    prelude::*,
    props::ExtractPrimaryKey,
    state::Store,
    widget::{
        data_table::{
            DataTable, DataTableColumn, DataTableHeader, DataTableKeyboardEvent,
            DataTableMouseEvent, DataTableRowRenderArgs,
        },
        Fa,
    },
};
use yew::{
    virtual_dom::{Key, VComp, VNode},
    Properties,
};

use crate::{pve::GuestType, search_provider::get_search_provider};

use super::loading_column;

#[derive(PartialEq, Clone, Properties)]
pub struct GuestPanel {
    guest_type: GuestType,
    status: Option<GuestStatusCount>,
}

impl GuestPanel {
    pub fn new(guest_type: GuestType, status: Option<GuestStatusCount>) -> Self {
        yew::props!(Self { guest_type, status })
    }
}

impl From<GuestPanel> for VNode {
    fn from(value: GuestPanel) -> Self {
        let comp = VComp::new::<PdmGuestPanel>(Rc::new(value), None);
        VNode::from(comp)
    }
}

#[derive(PartialEq, Clone)]
pub enum StatusRow {
    State(GuestState, u64),
    All(u64),
}

impl ExtractPrimaryKey for StatusRow {
    fn extract_key(&self) -> yew::virtual_dom::Key {
        yew::virtual_dom::Key::from(match self {
            StatusRow::State(state, _) => match state {
                GuestState::Running => "running",
                GuestState::Paused => "paused",
                GuestState::Stopped => "stopped",
                GuestState::Template => "template",
                GuestState::Unknown => "unknown",
            },
            StatusRow::All(_) => "all",
        })
    }
}

fn columns(guest_type: GuestType) -> Rc<Vec<DataTableHeader<StatusRow>>> {
    Rc::new(vec![
        DataTableColumn::new("icon")
            .width("3em")
            .render(move |item: &StatusRow| {
                match item {
                    StatusRow::State(state, _) => (*state).into(),
                    StatusRow::All(_) => match guest_type {
                        GuestType::Qemu => Fa::new("desktop"),
                        GuestType::Lxc => Fa::new("cubes"),
                    },
                }
                .fixed_width()
                .into()
            })
            .into(),
        DataTableColumn::new("text")
            .flex(5)
            .render(|item: &StatusRow| {
                match item {
                    StatusRow::State(GuestState::Running, _) => tr!("running"),
                    StatusRow::State(GuestState::Stopped, _) => tr!("stopped"),
                    StatusRow::State(GuestState::Paused, _) => tr!("paused"),
                    StatusRow::State(GuestState::Template, _) => tr!("Template"),
                    StatusRow::State(GuestState::Unknown, _) => tr!("Unknown"),
                    StatusRow::All(_) => tr!("All"),
                }
                .into()
            })
            .into(),
        DataTableColumn::new("count")
            .flex(1)
            .justify("right")
            .render(|item: &StatusRow| match item {
                StatusRow::State(_, count) => count.into(),
                StatusRow::All(count) => count.into(),
            })
            .into(),
    ])
}

pub struct PdmGuestPanel {}

impl yew::Component for PdmGuestPanel {
    type Message = Search;
    type Properties = GuestPanel;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self {}
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        if let Some(provider) = get_search_provider(ctx) {
            provider.search(msg);
        }
        false
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();
        if props.status.is_none() {
            return loading_column().into();
        }
        let guest_type = props.guest_type;
        let status = ctx.props().status.clone().unwrap();

        let store = Store::new();
        store.set_data(vec![
            StatusRow::State(GuestState::Running, status.running),
            StatusRow::State(GuestState::Stopped, status.stopped),
            StatusRow::State(GuestState::Template, status.template),
            StatusRow::State(GuestState::Unknown, status.unknown),
            StatusRow::All(status.running + status.stopped + status.template + status.unknown),
        ]);

        store.set_filter(|rec: &StatusRow| match rec {
            StatusRow::State(_, count) if *count > 0 => true,
            StatusRow::State(GuestState::Running | GuestState::Stopped, _) => true,
            StatusRow::All(_) => true,
            _ => false,
        });

        DataTable::new(columns(guest_type), store.clone())
            .padding(4)
            .striped(false)
            .borderless(true)
            .bordered(false)
            .row_render_callback(|renderer: &mut DataTableRowRenderArgs<StatusRow>| {
                renderer.class.push("pwt-pointer");
            })
            .on_row_keydown({
                let store = store.clone();
                let link = ctx.link().clone();
                move |event: &mut DataTableKeyboardEvent| match event.key().as_str() {
                    " " | "Enter" => search_callback(&link, &store, guest_type, &event.record_key),
                    _ => {}
                }
            })
            .on_row_click({
                let store = store.clone();
                let link = ctx.link().clone();
                move |event: &mut DataTableMouseEvent| {
                    search_callback(&link, &store, guest_type, &event.record_key);
                }
            })
            .show_header(false)
            .into()
    }
}

fn search_callback(
    link: &html::Scope<PdmGuestPanel>,
    store: &Store<StatusRow>,
    guest_type: GuestType,
    key: &Key,
) {
    if let Some((_, record)) = store.filtered_data().find(|(_, rec)| rec.key() == *key) {
        let (status, template) = match &*record.record() {
            StatusRow::State(guest_state, _) => match guest_state {
                GuestState::Running => (Some("running"), Some(false)),
                GuestState::Paused => (Some("paused"), Some(false)),
                GuestState::Stopped => (Some("stopped"), Some(false)),
                GuestState::Template => (None, Some(true)),
                GuestState::Unknown => (Some("unknown"), None),
            },
            StatusRow::All(_) => (None, None),
        };

        link.send_message(create_guest_search_term(guest_type, status, template));
    }
}

fn create_guest_search_term(
    guest_type: GuestType,
    status: Option<&'static str>,
    template: Option<bool>,
) -> Search {
    let resource_type: ResourceType = guest_type.into();
    if status.is_none() && template.is_none() {
        return Search::with_terms(vec![
            SearchTerm::new(resource_type.as_str()).category(Some("type"))
        ]);
    }

    let mut terms = vec![SearchTerm::new(resource_type.as_str()).category(Some("type"))];

    if let Some(template) = template {
        terms.push(SearchTerm::new(template.to_string()).category(Some("template")));
    }
    if let Some(status) = status {
        terms.push(SearchTerm::new(status).category(Some("status")));
    }
    Search::with_terms(terms)
}
