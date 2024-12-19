use std::rc::Rc;

use anyhow::{bail, Error};
use serde::{Deserialize, Serialize};
use serde_json::json;
use yew::html::IntoEventCallback;
use yew::virtual_dom::{Key, VComp, VNode};

use pwt::css::{AlignItems, FlexFit};
use pwt::widget::form::{Field, FormContext, FormContextObserver};
use pwt::widget::{error_message, Button, Column, Container, InputPanel, Mask, Row};
use pwt::{prelude::*, AsyncPool};

use proxmox_yew_comp::{SchemaValidation, WizardPageRenderInfo};

use pdm_api_types::remotes::RemoteType;
use pdm_api_types::CERT_FINGERPRINT_SHA256_SCHEMA;
use pdm_client::types::ListRealm;

use pwt_macros::builder;

#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct WizardPageConnect {
    info: WizardPageRenderInfo,

    #[builder_cb(IntoEventCallback, into_event_callback, Option<ConnectParams>)]
    #[prop_or_default]
    pub on_connect_change: Option<Callback<Option<ConnectParams>>>,

    remote_type: RemoteType,
}

impl WizardPageConnect {
    pub fn new(info: WizardPageRenderInfo, remote_type: RemoteType) -> Self {
        yew::props!(Self { info, remote_type })
    }
}

async fn list_realms(
    hostname: String,
    fingerprint: Option<String>,
) -> Result<Vec<ListRealm>, Error> {
    let mut params = json!({
        "hostname": hostname,
    });
    if let Some(fp) = fingerprint {
        params["fingerprint"] = fp.into();
    }
    let result: Vec<ListRealm> = proxmox_yew_comp::http_post("/pve/realms", Some(params)).await?;

    Ok(result)
}

#[derive(PartialEq, Clone, Deserialize, Serialize)]
/// Parameters for connect call.
pub struct ConnectParams {
    pub hostname: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(default)]
    pub realms: Vec<ListRealm>,
}

async fn connect(form_ctx: FormContext, remote_type: RemoteType) -> Result<ConnectParams, Error> {
    let data = form_ctx.get_submit_data();
    let mut data: ConnectParams = serde_json::from_value(data.clone())?;
    if let Some(hostname) = data.hostname.strip_prefix("http://") {
        data.hostname = hostname.to_string();
    }
    if let Some(hostname) = data.hostname.strip_prefix("https://") {
        data.hostname = hostname.to_string();
    }
    if let Some(hostname) = data.hostname.strip_suffix("/") {
        data.hostname = hostname.to_string();
    }

    let realms = match remote_type {
        RemoteType::Pve => list_realms(data.hostname.clone(), data.fingerprint.clone()).await?,
        RemoteType::Pbs => bail!("not implemented"),
    };

    data.realms = realms;
    Ok(data)
}

pub enum Msg {
    FormChange,
    Connect,
    ConnectResult(Result<ConnectParams, Error>),
}
pub struct PdmWizardPageConnect {
    connect_info: Option<ConnectParams>,
    _form_observer: FormContextObserver,
    form_valid: bool,
    loading: bool,
    last_error: Option<Error>,
    async_pool: AsyncPool,
}

impl PdmWizardPageConnect {
    fn update_connect_info(&mut self, ctx: &Context<Self>, info: Option<ConnectParams>) {
        let props = ctx.props();
        self.connect_info = info.clone();
        props.info.page_lock(info.is_none());
        if let Some(on_connect_change) = &props.on_connect_change {
            on_connect_change.emit(info);
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
            connect_info: None,
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
                        self.update_connect_info(ctx, None);
                    }
                    RemoteType::Pbs => {
                        return <Self as yew::Component>::update(self, ctx, Msg::Connect)
                    }
                }
            }
            Msg::Connect => {
                let link = ctx.link().clone();
                self.update_connect_info(ctx, None);
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
                    Ok(connect_info) => {
                        self.update_connect_info(ctx, Some(connect_info));
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
            .with_large_field(
                tr!("Server Address"),
                Field::new()
                    .name("hostname")
                    .placeholder(tr!("<IP/Hostname>:Port"))
                    .required(true),
            )
            .with_large_field(
                tr!("Fingerprint"),
                Field::new()
                    .name("fingerprint")
                    .placeholder(tr!("Server certificate SHA-256 fingerprint, required for self-signed certificates"))
                    .schema(&CERT_FINGERPRINT_SHA256_SCHEMA),
            );

        let content = Column::new()
            .class(FlexFit)
            .with_child(input_panel)
            .with_optional_child(
                (props.remote_type == RemoteType::Pve).then_some(
                    Row::new()
                        .padding(2)
                        .gap(2)
                        .class(AlignItems::Center)
                        .with_optional_child(
                            self.last_error
                                .as_deref()
                                .map(|err| error_message(&err.to_string())),
                        )
                        .with_flex_spacer()
                        .with_optional_child(
                            (self.last_error.is_none() && self.connect_info.is_some())
                                .then_some(Container::new().with_child(tr!("Connection OK"))),
                        )
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
