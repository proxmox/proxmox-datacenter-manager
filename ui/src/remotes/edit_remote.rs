use std::rc::Rc;

use yew::html::IntoEventCallback;
use yew::virtual_dom::{VComp, VNode};

use pwt::prelude::*;
use pwt::widget::form::{Field, FormContext, InputType};
use pwt::widget::{Container, InputPanel};

use proxmox_yew_comp::percent_encoding::percent_encode_component;
use proxmox_yew_comp::{EditWindow, SchemaValidation};

use proxmox_schema::ApiType;

use super::NodeUrlList;

use pwt_macros::builder;

#[derive(PartialEq, Properties)]
#[builder]
pub struct EditRemote {
    remote_id: String,
    /// Done callback, called after Close, Abort or Submit.
    #[builder_cb(IntoEventCallback, into_event_callback, ())]
    #[prop_or_default]
    pub on_done: Option<Callback<()>>,
}

impl EditRemote {
    pub fn new(remote_id: &str) -> Self {
        yew::props!(Self {
            remote_id: remote_id.to_owned()
        })
    }
}

pub struct PdmEditRemote {}

impl Component for PdmEditRemote {
    type Message = ();
    type Properties = EditRemote;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {}
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        EditWindow::new(tr!("Edit") + ": " + &tr!("Remote"))
            .min_height(400)
            .on_done(props.on_done.clone())
            .loader(format!(
                "/remotes/{}/config",
                percent_encode_component(&props.remote_id)
            ))
            .renderer(edit_remote_input_panel)
            .on_submit({
                let url = format!("/remotes/{}", percent_encode_component(&props.remote_id));
                move |form_ctx: FormContext| {
                    let url = url.clone();
                    async move {
                        let data = form_ctx.get_submit_data();
                        proxmox_yew_comp::http_put(&url, Some(data)).await?;
                        Ok(())
                    }
                }
            })
            .into()
    }
}

fn edit_remote_input_panel(_form_ctx: &FormContext) -> Html {
    InputPanel::new()
        .class("pwt-w-100")
        .padding(4)
        .with_field(tr!("Remote ID"), Field::new().name("id").required(true))
        .with_field(
            tr!("User/Token"),
            Field::new()
                .name("authid")
                .schema(&pdm_api_types::Authid::API_SCHEMA)
                .required(true),
        )
        .with_field(
            tr!("Password/Secret"),
            Field::new()
                .name("token")
                .input_type(InputType::Password)
                .required(true),
        )
        .with_custom_child(
            Container::new()
                .key("nodes-title")
                .padding_top(4)
                .class("pwt-font-title-medium")
                .with_child(tr!("Nodes")),
        )
        .with_custom_child(NodeUrlList::new().name("nodes").key("nodes").padding_top(2))
        .into()
}

impl Into<VNode> for EditRemote {
    fn into(self) -> VNode {
        let comp = VComp::new::<PdmEditRemote>(Rc::new(self), None);
        VNode::from(comp)
    }
}
