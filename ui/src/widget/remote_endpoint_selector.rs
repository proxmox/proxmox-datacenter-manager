use std::rc::Rc;

use wasm_bindgen::UnwrapThrowExt;
use yew::{
    html::{IntoEventCallback, IntoPropValue},
    AttrValue, Callback, Component, Properties,
};

use pwt::{
    props::{FieldBuilder, WidgetBuilder},
    widget::form::Combobox,
};
use pwt_macros::{builder, widget};

use crate::RemoteList;

#[widget(comp=PdmEndpointSelector, @input)]
#[derive(Clone, Properties, PartialEq)]
#[builder]
pub struct EndpointSelector {
    /// The default value
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub default: Option<AttrValue>,

    /// Change callback
    #[builder_cb(IntoEventCallback, into_event_callback, String)]
    #[prop_or_default]
    pub on_change: Option<Callback<String>>,

    /// The remote to list Endpoints from
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub remote: AttrValue,
}

impl EndpointSelector {
    pub fn new(remote: AttrValue) -> Self {
        yew::props!(Self { remote })
    }
}

pub struct PdmEndpointSelector {
    endpoints: Rc<Vec<AttrValue>>,
}

impl PdmEndpointSelector {
    fn update_endpoint_list(&mut self, ctx: &yew::Context<Self>) {
        let (remotes, _): (RemoteList, _) = ctx
            .link()
            .context(ctx.link().callback(|_| ()))
            .unwrap_throw();

        let remote_id = ctx.props().remote.as_str();

        for remote in remotes.iter() {
            if remote.id != remote_id {
                continue;
            }

            let endpoints = remote
                .nodes
                .iter()
                .map(|endpoint| AttrValue::from(endpoint.hostname.clone()))
                .collect();
            self.endpoints = Rc::new(endpoints);
            break;
        }
    }
}

impl Component for PdmEndpointSelector {
    type Message = ();
    type Properties = EndpointSelector;

    fn create(ctx: &yew::Context<Self>) -> Self {
        let mut this = Self {
            endpoints: Rc::new(Vec::new()),
        };

        this.update_endpoint_list(ctx);
        this
    }

    fn changed(&mut self, ctx: &yew::Context<Self>, old_props: &Self::Properties) -> bool {
        if ctx.props().remote != old_props.remote {
            self.update_endpoint_list(ctx);
        }
        true
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();
        Combobox::new()
            .with_std_props(&props.std_props)
            .with_input_props(&props.input_props)
            .on_change(props.on_change.clone())
            .default(props.default.clone())
            .items(self.endpoints.clone())
            .into()
    }
}
