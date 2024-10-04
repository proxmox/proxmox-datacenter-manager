use std::rc::Rc;

use anyhow::Error;
use pwt::css::ColorScheme;
use pwt::widget::form::Form;
use pwt::widget::Button;
use pwt::widget::Row;
use serde_json::Value;
use yew::prelude::Context as YewContext;
use yew::virtual_dom::VComp;
use yew::virtual_dom::VNode;

use pwt::prelude::*;
use pwt::widget::form::{Field, FormContext};
use pwt::widget::{InputPanel, Panel};

use proxmox_yew_comp::{ObjectGrid, ObjectGridRow};

#[function_component(WebauthnPanel)]
pub fn webauthn_panel() -> Html {
    Panel::new()
        .border(true)
        .title(tr!("WebAuthn TFA"))
        .with_child(object_grid())
        .into()
}

fn object_grid() -> ObjectGrid {
    ObjectGrid::new()
        .editable(true)
        .loader("/config/access/tfa/webauthn")
        .on_submit(store_tfa_config)
        .rows(ROWS.with(Rc::clone))
}

async fn store_tfa_config(form: FormContext) -> Result<(), Error> {
    let data = form.get_submit_data();
    proxmox_yew_comp::http_put("/config/access/tfa/webauthn", Some(data)).await
}

thread_local! {
    static ROWS: Rc<Vec<ObjectGridRow>> = Rc::new(vec![
        ObjectGridRow::new("rp", tr!("Relying Party"))
            .editor(webauthn_config_editor)
            .placeholder(tr!("Not configured")),
        ObjectGridRow::new("origin", tr!("Origin"))
            .editor(webauthn_config_editor)
            .placeholder(tr!("Not configured")),
        ObjectGridRow::new("id", tr!("ID"))
            .editor(webauthn_config_editor)
            .placeholder(tr!("Not configured")),
    ]);
}

fn webauthn_config_editor(
    form_ctx: &FormContext,
    _name: &str,
    _value: &Value,
    _record: &Value,
) -> Html {
    WebauthnConfigEditor::new(form_ctx.clone()).into()
}

#[derive(PartialEq, Properties)]
pub struct WebauthnConfigEditor {
    form_ctx: FormContext,
}

impl WebauthnConfigEditor {
    pub fn new(form_ctx: FormContext) -> Self {
        yew::props!(Self { form_ctx })
    }
}

impl Into<VNode> for WebauthnConfigEditor {
    fn into(self) -> VNode {
        VComp::new::<PdmWebauthnConfigEditor>(Rc::new(self), None).into()
    }
}

pub struct PdmWebauthnConfigEditor {
    form_ctx: FormContext,
}

pub enum Msg {
    AutoFill,
}

impl Component for PdmWebauthnConfigEditor {
    type Message = Msg;
    type Properties = WebauthnConfigEditor;

    fn create(ctx: &YewContext<Self>) -> Self {
        Self {
            form_ctx: ctx.props().form_ctx.clone(),
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::AutoFill => {
                let location = web_sys::window()
                    .unwrap()
                    .document()
                    .unwrap()
                    .location()
                    .unwrap();
                let hostname = location.hostname().unwrap();
                let origin = location.origin().unwrap();
                let mut form = self.form_ctx.write();
                form.set_field_value("rp", hostname.clone().into());
                form.set_field_value("origin", origin.into());
                form.set_field_value("id", hostname.into());
                true
            }
        }
    }

    fn view(&self, ctx: &YewContext<Self>) -> Html {
        let panel = InputPanel::new()
            .padding(4)
            .with_field(
                tr!("Relying Party"),
                Field::new().name("rp").required(true).autofocus(true),
            )
            .with_field(tr!("Origin"), Field::new().name("origin").required(true))
            .with_field(tr!("ID"), Field::new().name("id").required(true))
            .with_custom_child(
                Row::new().with_flex_spacer().with_child(
                    Button::new(tr!("Auto-fill"))
                        .class(ColorScheme::Primary)
                        .icon_class("fa fa-fw fa-pencil-square-o")
                        .onclick(ctx.link().callback(move |event: MouseEvent| {
                            event.stop_propagation();
                            Msg::AutoFill
                        })),
                ),
            );

        Form::new()
            .class("pwt-flex-fit")
            .form_context(self.form_ctx.clone())
            .with_child(panel)
            .into()
    }
}
