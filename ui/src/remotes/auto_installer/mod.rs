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

pub enum Msg {
    /// The prepared-answer flow created a new token, refresh the token panel.
    TokenCreated,
}

pub struct AutoInstallerPanelComponent {
    token_reload: usize,
}

impl Component for AutoInstallerPanelComponent {
    type Message = Msg;
    type Properties = AutoInstallerPanel;

    fn create(_: &Context<Self>) -> Self {
        Self { token_reload: 0 }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::TokenCreated => {
                self.token_reload = self.token_reload.wrapping_add(1);
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
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
                Panel::new().title(answers_title).with_child(
                    prepared_answers_panel::PreparedAnswersPanel::default()
                        .on_token_created(ctx.link().callback(|_| Msg::TokenCreated)),
                ),
            )
            .with_child(
                Panel::new().title(secrets_title).with_child(
                    token_panel::AuthTokenPanel::default().reload_trigger(self.token_reload),
                ),
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
