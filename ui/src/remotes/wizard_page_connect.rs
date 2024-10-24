use std::rc::Rc;

use anyhow::Error;
use serde::{Deserialize, Serialize};
use yew::html::IntoEventCallback;
use yew::virtual_dom::{Key, VComp, VNode};

use pwt::css::{AlignItems, FlexFit, FontColor};
use pwt::widget::form::{Field, FormContext, FormContextObserver, InputType};
use pwt::widget::{Button, Column, Fa, InputPanel, Mask, Row};
use pwt::{prelude::*, AsyncPool};

use proxmox_yew_comp::{SchemaValidation, WizardPageRenderInfo};

use proxmox_schema::property_string::PropertyString;
use proxmox_schema::ApiType;

use pdm_api_types::remotes::{NodeUrl, Remote, RemoteType};
use pdm_api_types::CERT_FINGERPRINT_SHA256_SCHEMA;

use pwt_macros::builder;

#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct WizardPageConnect {
    info: WizardPageRenderInfo,

    #[builder_cb(IntoEventCallback, into_event_callback, Option<Remote>)]
    #[prop_or_default]
    pub on_server_change: Option<Callback<Option<Remote>>>,

    remote_type: RemoteType,
}

impl WizardPageConnect {
    pub fn new(info: WizardPageRenderInfo, remote_type: RemoteType) -> Self {
        yew::props!(Self { info, remote_type })
    }
}

async fn scan(connect: ConnectParams) -> Result<Remote, Error> {
    let params = serde_json::to_value(&connect)?;
    let mut result: Remote = proxmox_yew_comp::http_post("/pve/scan", Some(params)).await?;

    // insert the initial connection too, since we know that works
    result.nodes.insert(
        0,
        PropertyString::new(NodeUrl {
            hostname: connect.hostname,
            fingerprint: connect.fingerprint,
        }),
    );

    result.nodes.sort_by(|a, b| a.hostname.cmp(&b.hostname));

    Ok(result)
}

#[derive(Deserialize, Serialize)]
/// Parameters for connect call.
pub struct ConnectParams {
    hostname: String,
    authid: String,
    token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    fingerprint: Option<String>,
}

async fn connect(form_ctx: FormContext, remote_type: RemoteType) -> Result<Remote, Error> {
    let data = form_ctx.get_submit_data();
    let data: ConnectParams = serde_json::from_value(data.clone())?;

    Ok(match remote_type {
        RemoteType::Pve => scan(data).await?,
        RemoteType::Pbs => Remote {
            ty: remote_type,
            id: data.hostname.clone(),
            authid: data.authid.parse()?,
            token: data.token,
            nodes: vec![PropertyString::new(NodeUrl {
                hostname: data.hostname,
                fingerprint: data.fingerprint,
            })],
        },
    })
}

pub enum Msg {
    FormChange,
    Connect,
    ConnectResult(Result<Remote, Error>),
}
pub struct PdmWizardPageConnect {
    server_info: Option<Remote>,
    _form_observer: FormContextObserver,
    form_valid: bool,
    loading: bool,
    last_error: Option<Error>,
    async_pool: AsyncPool,
}

impl PdmWizardPageConnect {
    fn update_server_info(&mut self, ctx: &Context<Self>, server_info: Option<Remote>) {
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
            last_error: None,
            async_pool: AsyncPool::new(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let props = ctx.props();
        match msg {
            Msg::FormChange => {
                self.form_valid = props.info.form_ctx.read().is_valid();
                match props.remote_type {
                    RemoteType::Pve => {
                        self.update_server_info(ctx, None);
                    }
                    RemoteType::Pbs => {
                        return <Self as yew::Component>::update(self, ctx, Msg::Connect)
                    }
                }
            }
            Msg::Connect => {
                let link = ctx.link().clone();
                self.update_server_info(ctx, None);
                let form_ctx = props.info.form_ctx.clone();
                self.loading = true;
                self.last_error = None;

                let remote_type = props.remote_type;
                self.async_pool.spawn(async move {
                    let result = connect(form_ctx, remote_type).await;
                    link.send_message(Msg::ConnectResult(result));
                });
            }
            Msg::ConnectResult(server_info) => {
                self.loading = false;
                match server_info {
                    Ok(server_info) => {
                        self.update_server_info(ctx, Some(server_info));
                    }
                    Err(err) => {
                        self.last_error = Some(err);
                    }
                }

                if let Some(form_ctx) = props.info.lookup_form_context(&Key::from("nodes")) {
                    form_ctx.write().reset_form();
                }
                props.info.reset_remaining_valid_pages();
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let input_panel = InputPanel::new()
            .class(FlexFit)
            // FIXME: input panel css style is not optimal here...
            .width("auto")
            .padding(4)
            .with_field(
                tr!("Server address"),
                Field::new().name("hostname").required(true),
            )
            .with_right_field(
                tr!("User/Token"),
                Field::new()
                    .name("authid")
                    .schema(&pdm_api_types::Authid::API_SCHEMA)
                    .required(true),
            )
            .with_right_field(
                tr!("Password/Secret"),
                Field::new()
                    .name("token")
                    .input_type(InputType::Password)
                    .required(true),
            )
            .with_large_field(
                tr!("Fingerprint"),
                Field::new()
                    .name("fingerprint")
                    .schema(&CERT_FINGERPRINT_SHA256_SCHEMA),
            );

        let content = Column::new()
            .class(FlexFit)
            .with_child(input_panel)
            .with_optional_child(
                (props.remote_type == RemoteType::Pve).then_some(
                    Row::new()
                        .padding(2)
                        .with_optional_child(self.last_error.as_deref().map(|err| {
                            Row::new()
                                .class(AlignItems::Center)
                                .gap(4)
                                .with_child(
                                    Fa::new("exclamation-triangle")
                                        .large()
                                        .class(FontColor::Error),
                                )
                                .with_child(err.to_string())
                        }))
                        .with_flex_spacer()
                        .with_child(
                            Button::new("Connect")
                                .disabled(!self.form_valid)
                                .onclick(ctx.link().callback(|_| Msg::Connect)),
                        ),
                ),
            );
        Mask::new(content)
            .class(FlexFit)
            .visible(self.loading)
            .into()
    }
}

impl Into<VNode> for WizardPageConnect {
    fn into(self) -> VNode {
        let comp = VComp::new::<PdmWizardPageConnect>(Rc::new(self), None);
        VNode::from(comp)
    }
}
