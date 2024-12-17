use std::rc::Rc;

use gloo_utils::{document, window};
use wasm_bindgen::{prelude::Closure, JsCast};
use web_sys::{Element, HtmlElement};
use yew::{
    virtual_dom::{VComp, VNode},
    Component, NodeRef, Properties,
};

use pwt::{
    dom::focus::FocusTracker,
    dom::IntoHtmlElement,
    prelude::*,
    props::CssLength,
    widget::{form::Field, Container},
};

use super::ResourceTree;

#[derive(Properties, PartialEq)]
pub struct SearchBox {}

impl SearchBox {
    pub fn new() -> Self {
        Self {}
    }
}

impl From<SearchBox> for VNode {
    fn from(val: SearchBox) -> Self {
        let comp = VComp::new::<PdmSearchBox>(Rc::new(val), None);
        VNode::from(comp)
    }
}

pub enum Msg {
    ChangeTerm(String),
    FocusChange(bool),
    ToggleFocus,
}

pub struct PdmSearchBox {
    search_field_ref: NodeRef,
    search_box_ref: NodeRef,
    search_term: String,
    focus_tracker: FocusTracker,
    focus: bool,
    global_shortcut_listener: Closure<dyn Fn(KeyboardEvent)>,
    toggle_focus: bool,
}

impl Component for PdmSearchBox {
    type Message = Msg;

    type Properties = SearchBox;

    fn create(ctx: &yew::Context<Self>) -> Self {
        let link = ctx.link().clone();
        Self {
            search_field_ref: Default::default(),
            search_box_ref: Default::default(),
            search_term: String::new(),
            focus_tracker: FocusTracker::new(ctx.link().callback(Msg::FocusChange)),
            focus: false,
            toggle_focus: false,
            global_shortcut_listener: Closure::wrap(Box::new(move |e: KeyboardEvent| {
                match (e.ctrl_key(), e.shift_key(), e.code().as_str()) {
                    (true, _, "Space") | (true, true, "KeyF") => {
                        link.send_message(Msg::ToggleFocus);
                    }
                    _ => {}
                }
            })),
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::ChangeTerm(term) => {
                self.search_term = term;
                true
            }
            Msg::FocusChange(focus) => {
                let changed = self.focus != focus;
                self.focus = focus;
                changed
            }
            Msg::ToggleFocus => {
                self.toggle_focus = true;
                true
            }
        }
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let search_result = ResourceTree::new()
            .node_ref(self.search_box_ref.clone())
            .search_term(self.search_term.clone())
            .search_only(true)
            .style("position", "absolute")
            .style("z-index", "100")
            .style(
                "display",
                if self.focus && !self.search_term.is_empty() {
                    ""
                } else {
                    "none"
                },
            )
            .border(true)
            .width(CssLength::Fraction(0.5))
            .height(400)
            .class("pwt-shadow2");

        Container::new()
            .onfocusin(self.focus_tracker.get_focus_callback(true))
            .onfocusout(self.focus_tracker.get_focus_callback(false))
            .flex(2.0)
            .min_width(230) // placeholder text
            .with_child(
                Field::new()
                    .placeholder(tr!("Search (Ctrl+Space / Ctrl+Shift+F)"))
                    .node_ref(self.search_field_ref.clone())
                    .on_input(ctx.link().callback(Msg::ChangeTerm)),
            )
            .with_child(search_result)
            .into()
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render
            && window()
                .add_event_listener_with_callback(
                    "keydown",
                    self.global_shortcut_listener.as_ref().unchecked_ref(),
                )
                .is_err()
        {
            log::error!("could not attach global keydown listener");
        }

        if self.toggle_focus {
            // FIXME: ugly conversion/query code, we should be able to make that more readable
            if self.focus {
                if let Some(Ok(el)) = document()
                    .active_element()
                    .map(|el: Element| el.dyn_into::<HtmlElement>())
                {
                    let _ = el.blur();
                }
            } else if let Some(el) = self.search_field_ref.clone().into_html_element() {
                if let Ok(Some(el)) = el.query_selector("input") {
                    if let Ok(el) = el.dyn_into::<HtmlElement>() {
                        let _ = el.focus();
                    }
                }
            }

            self.toggle_focus = false;
        }
    }

    fn destroy(&mut self, _ctx: &Context<Self>) {
        if window()
            .remove_event_listener_with_callback(
                "keydown",
                self.global_shortcut_listener.as_ref().unchecked_ref(),
            )
            .is_err()
        {
            log::error!("could not remove global keydown listener");
        };
    }
}
