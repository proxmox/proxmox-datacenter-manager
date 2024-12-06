use std::rc::Rc;

use wasm_bindgen::UnwrapThrowExt;
use yew::{
    html::{IntoEventCallback, IntoPropValue},
    AttrValue, Callback, Component, Properties,
};

use pdm_api_types::remotes::RemoteType;
use pwt::{
    props::{FieldBuilder, WidgetBuilder},
    widget::form::Combobox,
};
use pwt_macros::{builder, widget};

use crate::RemoteList;

#[widget(comp=PdmRemoteSelector, @input)]
#[derive(Clone, Properties, PartialEq)]
#[builder]
pub struct RemoteSelector {
    /// The default value
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub default: Option<AttrValue>,

    /// Change callback
    #[builder_cb(IntoEventCallback, into_event_callback, String)]
    #[prop_or_default]
    pub on_change: Option<Callback<String>>,

    /// Limit the list to the given [`RemoteType`]
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub remote_type: Option<RemoteType>,
}

impl RemoteSelector {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

pub struct PdmRemoteSelector {
    remotes: Rc<Vec<AttrValue>>,
}

impl PdmRemoteSelector {
    fn update_remote_list(&mut self, ctx: &yew::Context<Self>) {
        let (remotes, _): (RemoteList, _) = ctx
            .link()
            .context(ctx.link().callback(|_| ()))
            .unwrap_throw();

        let ty = ctx.props().remote_type;

        let remotes = remotes
            .iter()
            .filter_map(move |remote| match (ty, remote.ty) {
                (Some(a), b) if a == b => Some(remote.id.clone().into()),
                (None, _) => Some(remote.id.clone().into()),
                _ => None,
            })
            .collect();

        self.remotes = Rc::new(remotes);
    }
}

impl Component for PdmRemoteSelector {
    type Message = ();
    type Properties = RemoteSelector;

    fn create(ctx: &yew::Context<Self>) -> Self {
        let mut this = Self {
            remotes: Rc::new(Vec::new()),
        };

        this.update_remote_list(ctx);
        this
    }

    fn changed(&mut self, ctx: &yew::Context<Self>, _old_props: &Self::Properties) -> bool {
        if ctx.props().remote_type != _old_props.remote_type {
            self.update_remote_list(ctx);
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
            .items(self.remotes.clone())
            .into()
    }
}
