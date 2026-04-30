//! Implements the UI for the proxmox-auto-installer integration.

mod installations_panel;

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

        Container::new()
            .class("pwt-content-spacer")
            .class(Fit)
            .class(css::Display::Grid)
            .with_child(
                Panel::new()
                    .title(installations_title)
                    .with_child(installations_panel::InstallationsPanel::default()),
            )
            .into()
    }
}
