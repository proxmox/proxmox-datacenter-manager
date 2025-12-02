use std::rc::Rc;

use anyhow::Error;
use serde_json::Value;
use yew::html::IntoEventCallback;
use yew::virtual_dom::{VComp, VNode};

use pwt::css::FlexFit;
use pwt::prelude::*;
use pwt::widget::form::{DisplayField, Field, FormContext, InputType};
use pwt::widget::{Container, InputPanel};

use proxmox_yew_comp::form::delete_empty_values;
use proxmox_yew_comp::percent_encoding::percent_encode_component;
use proxmox_yew_comp::{EditWindow, SchemaValidation};

use proxmox_client::ApiResponseData;
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

async fn load_remote(url: AttrValue) -> Result<ApiResponseData<Value>, Error> {
    proxmox_yew_comp::http_get_full(&*url, None).await
}

impl Component for PdmEditRemote {
    type Message = ();
    type Properties = EditRemote;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {}
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let url = format!(
            "/remotes/remote/{}/config",
            percent_encode_component(&props.remote_id)
        );
        EditWindow::new(tr!("Edit") + ": " + &tr!("Remote"))
            .width(800)
            .min_height(400)
            .on_done(props.on_done.clone())
            .loader((load_remote, url))
            .renderer({
                let remote_id = props.remote_id.clone();
                move |form_ctx| edit_remote_input_panel(form_ctx, &remote_id)
            })
            .on_submit({
                let url = format!(
                    "/remotes/remote/{}",
                    percent_encode_component(&props.remote_id)
                );
                move |form_ctx: FormContext| {
                    let url = url.clone();
                    async move {
                        let data = form_ctx.get_submit_data();

                        let data = delete_empty_values(&data, &["web-url"], true);

                        proxmox_yew_comp::http_put(&url, Some(data)).await
                    }
                }
            })
            .into()
    }
}

fn edit_remote_input_panel(_form_ctx: &FormContext, remote_id: &str) -> Html {
    InputPanel::new()
        .class(FlexFit)
        .padding(4)
        .width("auto")
        .with_field(
            tr!("Remote ID"),
            DisplayField::new()
                .value(remote_id.to_string())
                .key("remote-id"),
        )
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
                .placeholder(tr!("Unchanged"))
                .input_type(InputType::Password)
                .required(false),
        )
        .with_field(
            tr!("Web UI URL"),
            Field::new()
                .name("web-url")
                .placeholder(tr!("Use first endpoint.")),
        )
        .with_custom_child(
            Container::new()
                .key("nodes-title")
                .padding_top(4)
                .class("pwt-font-title-medium")
                .with_child(tr!("Endpoints")),
        )
        .with_custom_child(NodeUrlList::new().name("nodes").key("nodes").padding_top(2))
        .into()
}

impl From<EditRemote> for VNode {
    fn from(val: EditRemote) -> Self {
        let comp = VComp::new::<PdmEditRemote>(Rc::new(val), None);
        VNode::from(comp)
    }
}
