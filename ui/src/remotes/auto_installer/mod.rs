//! Implements the UI for the proxmox-auto-installer integration.

mod installations_panel;
mod prepared_answer_add_wizard;
mod prepared_answer_edit_window;
mod prepared_answer_form;
mod prepared_answers_panel;
mod token_panel;
mod token_selector;

use std::rc::Rc;
use yew::virtual_dom::{VComp, VNode};

use pwt::{
    css::{self, AlignItems, Fit},
    prelude::*,
    props::{ContainerBuilder, WidgetBuilder},
    widget::{Container, Fa, Panel, Row},
};

#[derive(Default, PartialEq, Properties)]
pub struct AutoInstallerPanel {}

impl From<AutoInstallerPanel> for VNode {
    fn from(value: AutoInstallerPanel) -> Self {
        VComp::new::<AutoInstallerPanelComponent>(Rc::new(value), None).into()
    }
}

pub struct AutoInstallerPanelComponent {}

impl Component for AutoInstallerPanelComponent {
    type Message = ();
    type Properties = AutoInstallerPanel;

    fn create(_: &Context<Self>) -> Self {
        Self {}
    }

    fn view(&self, _: &Context<Self>) -> Html {
        let installations_title: Html = Row::new()
            .gap(2)
            .class(AlignItems::Baseline)
            .with_child(Fa::new("cubes"))
            .with_child(tr!("Installations"))
            .into();

        let answers_title: Html = Row::new()
            .gap(2)
            .class(AlignItems::Baseline)
            .with_child(Fa::new("files-o"))
            .with_child(tr!("Prepared Answers"))
            .into();

        let secrets_title: Html = Row::new()
            .gap(2)
            .class(AlignItems::Baseline)
            .with_child(Fa::new("key"))
            .with_child(tr!("Authentication Tokens"))
            .into();

        Container::new()
            .class("pwt-content-spacer")
            .class(Fit)
            .class(css::Display::Grid)
            .style("grid-template-columns", "repeat(2, 1fr)")
            .style("grid-template-rows", "repeat(2, 1fr)")
            .with_child(
                Panel::new()
                    .style("grid-row", "span 2 / span 2")
                    .title(installations_title)
                    .with_child(installations_panel::InstallationsPanel::default()),
            )
            .with_child(
                Panel::new()
                    .title(answers_title)
                    .with_child(prepared_answers_panel::PreparedAnswersPanel::default()),
            )
            .with_child(
                Panel::new()
                    .title(secrets_title)
                    .with_child(token_panel::AuthTokenPanel::default()),
            )
            .into()
    }
}

fn pdm_origin() -> Option<String> {
    gloo_utils::document()
        .url()
        .and_then(|s| web_sys::Url::new(&s))
        .map(|url| url.origin())
        .ok()
}
