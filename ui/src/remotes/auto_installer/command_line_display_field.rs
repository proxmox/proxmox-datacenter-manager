//! A hidden-by-default [`TextArea`] to display the generated command line for
//! `proxmox-auto-install-assistant`.

use yew::Properties;

use proxmox_yew_comp::utils::copy_text_to_clipboard;
use pwt::{
    css::{self, ColorScheme},
    prelude::*,
    widget::{Button, Column, Row, Tooltip, form::TextArea},
};
use pwt_macros::{builder, widget};

#[widget(comp=CommandLineDisplayFieldComponent, @input)]
#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct CommandLineDisplayField {
    /// Content to display.
    #[prop_or_default]
    #[builder]
    value: AttrValue,
}

impl CommandLineDisplayField {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

enum Message {
    Reveal,
    Hide,
}

struct CommandLineDisplayFieldComponent {
    hidden: bool,
}

impl Component for CommandLineDisplayFieldComponent {
    type Properties = CommandLineDisplayField;
    type Message = Message;

    fn create(_ctx: &Context<Self>) -> Self {
        Self { hidden: true }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let hidden = self.hidden;

        let toggle_hidden_cb = ctx.link().callback(move |_| {
            if hidden {
                Self::Message::Reveal
            } else {
                Self::Message::Hide
            }
        });

        let (toggle_hidden_icon, toggle_hidden_tip, value) = if hidden {
            (
                "fa fa-eye",
                tr!("Show Command Line"),
                "\u{2022}".repeat(props.value.len()).into(),
            )
        } else {
            (
                "fa fa-eye-slash",
                tr!("Hide Command Line"),
                props.value.clone(),
            )
        };

        Row::new()
            .with_std_props(&props.std_props)
            .class(css::Display::Flex)
            .class("pwt-fill-grid-row")
            .gap(2)
            .with_child(
                TextArea::new()
                    .attribute("rows", "10")
                    .attribute("resize", "both")
                    .read_only(true)
                    .class("pwt-w-100")
                    .value(value)
            )
            .with_child(
                    Column::new().gap(2).with_child(
                    Tooltip::new(
                            Button::new_icon(toggle_hidden_icon)
                                .class(ColorScheme::Primary)
                                .on_activate(toggle_hidden_cb)
                    )
                    .tip(toggle_hidden_tip),
                )
                .with_child(
                        Tooltip::new(
                            Button::new_icon("fa fa-clipboard")
                                .class(ColorScheme::Primary)
                                .on_activate({
                                    let value = props.value.clone();
                                    move |_| copy_text_to_clipboard(&value)
                                }),
                        )
                        .tip(tr!("Copy template command line to clipboard. Replace INPUT.iso with your installation ISO.")),
                )
            )
            .into()
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Self::Message::Reveal => self.hidden = false,
            Self::Message::Hide => self.hidden = true,
        }

        true
    }
}
