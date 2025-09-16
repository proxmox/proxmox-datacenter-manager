use std::rc::Rc;

use yew::html::IntoPropValue;

use pwt::prelude::*;
use pwt::widget::form::Combobox;

use pwt_macros::{builder, widget};

static PREDEFINED_PATHS: &[&str] = &[
    "/",
    "/access",
    "/access/acl",
    "/access/users",
    "/resource",
    "/system",
    "/system/certificates",
    "/system/disks",
    "/system/log",
    "/system/notifications",
    "/system/network",
    "/system/network/dns",
    "/system/network/interfaces",
    "/system/services",
    "/system/status",
    "/system/tasks",
    "/system/time",
    "/system/services",
];

#[widget(comp=PdmPermissionPathSelector, @input, @element)]
#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct PermissionPathSelector {
    /// Default value
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    default: Option<AttrValue>,
}

impl PermissionPathSelector {
    pub(super) fn new() -> Self {
        yew::props!(Self {})
    }
}

enum Msg {}

struct PdmPermissionPathSelector {
    items: Rc<Vec<AttrValue>>,
}

impl PdmPermissionPathSelector {}

impl Component for PdmPermissionPathSelector {
    type Message = Msg;
    type Properties = PermissionPathSelector;

    fn create(_ctx: &Context<Self>) -> Self {
        // TODO: fetch resources & remotes from the backend to improve the pre-defined selection of
        // acl paths
        Self {
            items: Rc::new(
                PREDEFINED_PATHS
                    .iter()
                    .map(|i| AttrValue::from(*i))
                    .collect(),
            ),
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, _msg: Self::Message) -> bool {
        false
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        Combobox::new()
            .with_std_props(&props.std_props)
            .with_input_props(&props.input_props)
            .default(props.default.clone())
            .items(Rc::clone(&self.items))
            .editable(true)
            .into()
    }
}
