use std::rc::Rc;

use anyhow::Error;
use yew::html::IntoEventCallback;
use yew::virtual_dom::{VComp, VNode};

use pwt::prelude::*;
use pwt::widget::form::{Field, FormContext, FormContextObserver, InputType};
use pwt::widget::{Button, Column, InputPanel, Mask, Row};

use proxmox_yew_comp::{SchemaValidation, WizardPageRenderInfo};

use pdm_api_types::remotes::{NodeUrl, RemoteType};
use pdm_api_types::CERT_FINGERPRINT_SHA256_SCHEMA;

use pwt_macros::builder;

use super::{ConnectParams, ServerInfo};

#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct WizardPageConnect {
    info: WizardPageRenderInfo,

    #[builder_cb(IntoEventCallback, into_event_callback, Option<ServerInfo>)]
    #[prop_or_default]
    pub on_server_change: Option<Callback<Option<ServerInfo>>>,
}

impl WizardPageConnect {
    pub fn new(info: WizardPageRenderInfo) -> Self {
        yew::props!(Self { info })
    }
}

async fn connect(form_ctx: FormContext) -> Result<ServerInfo, Error> {
    let data = form_ctx.get_submit_data();
    let _data: ConnectParams = serde_json::from_value(data)?;

    // fixme: do real api call
    let future: wasm_bindgen_futures::JsFuture = proxmox_yew_comp::async_sleep(1000).into();
    let _ = future.await;

    Ok(ServerInfo {
        remote_type: RemoteType::Pve,
        nodes: vec![
            NodeUrl {
                hostname: String::from("node1"),
                fingerprint: None,
            },
            NodeUrl {
                hostname: String::from("node2"),
                fingerprint: None,
            },
        ],
    })
}

pub enum Msg {
    FormChange,
    Connect,
    ConnectResult(Result<ServerInfo, Error>),
}
pub struct PdmWizardPageConnect {
    server_info: Option<ServerInfo>,
    _form_observer: FormContextObserver,
    form_valid: bool,
    loading: bool,
}

impl PdmWizardPageConnect {
    fn update_server_info(&mut self, ctx: &Context<Self>, server_info: Option<ServerInfo>) {
        let props = ctx.props();
        self.server_info = server_info;
        props.info.page_lock(self.server_info.is_none());
        if let Some(on_server_change) = &props.on_server_change {
            on_server_change.emit(self.server_info.clone());
        }
    }
}
impl Component for PdmWizardPageConnect {
    type Message = Msg;
    type Properties = WizardPageConnect;

    fn create(ctx: &Context<Self>) -> Self {
        let props = ctx.props();

        let _form_observer = props
            .info
            .form_ctx
            .add_listener(ctx.link().callback(|_| Msg::FormChange));

        props.info.page_lock(true);

        Self {
            server_info: None,
            _form_observer,
            form_valid: false,
            loading: false,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let props = ctx.props();
        match msg {
            Msg::FormChange => {
                self.form_valid = props.info.form_ctx.read().is_valid();
                self.update_server_info(ctx, None);
            }
            Msg::Connect => {
                let link = ctx.link().clone();
                self.update_server_info(ctx, None);
                let form_ctx = props.info.form_ctx.clone();
                self.loading = true;
                wasm_bindgen_futures::spawn_local(async move {
                    let result = connect(form_ctx).await;
                    link.send_message(Msg::ConnectResult(result));
                });
            }
            Msg::ConnectResult(server_info) => {
                self.loading = false;
                match server_info {
                    Ok(server_info) => {
                        self.update_server_info(ctx, Some(server_info));
                    }
                    Err(_err) => {
                        todo!();
                    }
                }
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let input_panel = InputPanel::new()
            .padding(4)
            .with_field(
                tr!("Server address"),
                Field::new().name("server").required(true),
            )
            .with_right_field(
                tr!("Fingerprint"),
                Field::new()
                    .name("fingerprint")
                    .schema(&CERT_FINGERPRINT_SHA256_SCHEMA),
            )
            .with_field(
                tr!("User/Token"),
                Field::new().name("username").required(true),
            )
            .with_field(
                tr!("Password/Secret"),
                Field::new()
                    .name("password")
                    .input_type(InputType::Password)
                    .required(true),
            );

        let content = Column::new().with_child(input_panel).with_child(
            Row::new().padding(4).with_flex_spacer().with_child(
                Button::new("Connect")
                    .disabled(!self.form_valid)
                    .onclick(ctx.link().callback(|_| Msg::Connect)),
            ),
        );
        Mask::new(content).visible(self.loading).into()
    }
}

impl Into<VNode> for WizardPageConnect {
    fn into(self) -> VNode {
        let comp = VComp::new::<PdmWizardPageConnect>(Rc::new(self), None);
        VNode::from(comp)
    }
}
