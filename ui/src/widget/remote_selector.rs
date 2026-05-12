use std::rc::Rc;

use wasm_bindgen::UnwrapThrowExt;
use yew::{
    html::{IntoEventCallback, IntoPropValue},
    AttrValue, Callback, Component, ContextHandle, Properties,
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
    /// Forced value
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub value: Option<AttrValue>,

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

    /// Remote IDs to drop from the list (e.g. remotes with no node left to assign a key to).
    #[prop_or_default]
    pub excluded_remotes: Rc<Vec<AttrValue>>,
}

impl RemoteSelector {
    pub fn new() -> Self {
        yew::props!(Self {})
    }

    pub fn excluded_remotes(mut self, remotes: Rc<Vec<AttrValue>>) -> Self {
        self.excluded_remotes = remotes;
        self
    }
}

pub struct PdmRemoteSelector {
    remotes: Rc<Vec<AttrValue>>,
    _remotes_update_ctx: Option<ContextHandle<RemoteList>>,
}

impl PdmRemoteSelector {
    fn update_remote_list(&mut self, ctx: &yew::Context<Self>) {
        let (remotes, _remotes_update_ctx): (RemoteList, _) = ctx
            .link()
            .context(ctx.link().callback(|list| list))
            .unwrap_throw();

        self.set_remote_list(ctx, remotes);
        self._remotes_update_ctx = Some(_remotes_update_ctx);
    }

    fn set_remote_list(&mut self, ctx: &yew::Context<Self>, remotes: RemoteList) {
        let ty = ctx.props().remote_type;
        let excluded = ctx.props().excluded_remotes.clone();
        let remotes = remotes
            .iter()
            .filter_map(move |remote| {
                let id: AttrValue = remote.id.clone().into();
                if excluded.contains(&id) {
                    return None;
                }
                match (ty, remote.ty) {
                    (Some(a), b) if a == b => Some(id),
                    (None, _) => Some(id),
                    _ => None,
                }
            })
            .collect();

        self.remotes = Rc::new(remotes);
    }
}

impl Component for PdmRemoteSelector {
    type Message = RemoteList;
    type Properties = RemoteSelector;

    fn create(ctx: &yew::Context<Self>) -> Self {
        let mut this = Self {
            remotes: Rc::new(Vec::new()),
            _remotes_update_ctx: None,
        };

        this.update_remote_list(ctx);
        this
    }

    fn update(&mut self, ctx: &yew::Context<Self>, msg: Self::Message) -> bool {
        self.set_remote_list(ctx, msg);
        true
    }

    fn changed(&mut self, ctx: &yew::Context<Self>, _old_props: &Self::Properties) -> bool {
        if ctx.props().remote_type != _old_props.remote_type
            || ctx.props().excluded_remotes != _old_props.excluded_remotes
        {
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
            .value(props.value.clone())
            .items(self.remotes.clone())
            .into()
    }
}
