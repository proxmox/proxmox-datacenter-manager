use std::rc::Rc;

use pdm_api_types::resource::GuestStatusCount;
use proxmox_yew_comp::GuestState;
use pwt::{
    prelude::*,
    props::ExtractPrimaryKey,
    state::Store,
    widget::{
        data_table::{DataTable, DataTableColumn, DataTableHeader},
        Fa,
    },
};
use yew::virtual_dom::{VComp, VNode};

use crate::pve::GuestType;

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
    type Message = String;
    type Properties = GuestPanel;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self {}
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
            .show_header(false)
            .into()
    }
}
