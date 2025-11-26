use std::rc::Rc;

use pwt::prelude::*;
use pwt::state::Store;
use pwt::widget::form::Combobox;
use pwt_macros::{builder, widget};

use pdm_api_types::views::ViewConfig;

#[widget(comp=ViewSelectorComp, @input)]
#[derive(Clone, Properties, PartialEq)]
#[builder]
pub struct ViewSelector {
    store: Store<ViewConfig>,
}

impl ViewSelector {
    pub fn new(store: Store<ViewConfig>) -> Self {
        yew::props!(Self { store })
    }
}

#[doc(hidden)]
pub struct ViewSelectorComp {}

impl Component for ViewSelectorComp {
    type Message = ();
    type Properties = ViewSelector;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {}
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let mut list = vec!["__dashboard__".into()];
        let store = &ctx.props().store;
        for item in store.read().data().iter() {
            list.push(item.id.clone().into());
        }
        Combobox::new()
            .items(Rc::new(list))
            .with_input_props(&ctx.props().input_props)
            .on_change(|_| {})
            .render_value({
                move |value: &AttrValue| {
                    if value == "__dashboard__" {
                        html! {{tr!("Dashboard")}}
                    } else {
                        html! {{value}}
                    }
                }
            })
            .into()
    }
}
