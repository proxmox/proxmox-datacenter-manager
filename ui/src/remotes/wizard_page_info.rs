use std::rc::Rc;

use anyhow::Error;
use html::IntoEventCallback;
use proxmox_schema::property_string::PropertyString;
use serde::{Deserialize, Serialize};
use yew::virtual_dom::{Key, VComp, VNode};

use proxmox_yew_comp::WizardPageRenderInfo;
use pwt::{
    css::{self, FlexFit},
    prelude::*,
    widget::{
        error_message,
        form::{Combobox, Field, FormContext, FormContextObserver, InputType, RadioButton},
        Button, Column, Container, InputPanel, Mask, Row,
    },
    AsyncPool,
};

use pdm_api_types::remotes::{NodeUrl, Remote};

use pwt_macros::builder;

use super::wizard_page_connect::ConnectParams;

#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct WizardPageInfo {
    info: WizardPageRenderInfo,

    #[builder_cb(IntoEventCallback, into_event_callback, Option<Remote>)]
    #[prop_or_default]
    pub on_server_change: Option<Callback<Option<Remote>>>,

    #[builder]
    #[prop_or_default]
    connect_info: Option<ConnectParams>,
}

impl WizardPageInfo {
    pub fn new(info: WizardPageRenderInfo) -> Self {
        yew::props!(Self { info })
    }
}

pub struct PdmWizardPageInfo {
    user_mode: bool,
    realms: Rc<Vec<AttrValue>>,
    server_info: Option<Remote>,
    last_error: Option<Error>,
    credentials: Option<(String, String)>,
    loading: bool,
    _form_observer: FormContextObserver,
    async_pool: AsyncPool,
}

pub enum Msg {
    ToggleCreateToken(bool),
    FormChange,
    Connect,
    ConnectResult(Result<Remote, Error>),
}

#[derive(Deserialize, Serialize)]
/// Parameters for connect call.
pub struct ScanParams {
    hostname: String,
    authid: String,
    token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    fingerprint: Option<String>,
}

fn create_realm_list(props: &WizardPageInfo) -> Rc<Vec<AttrValue>> {
    if let Some(info) = &props.connect_info {
        let realms = Rc::new(
            info.realms
                .iter()
                .map(|realm| AttrValue::from(realm.realm.clone()))
                .collect(),
        );
        realms
    } else {
        Rc::new(Vec::new())
    }
}

async fn scan(connection_params: ConnectParams, form_ctx: FormContext) -> Result<Remote, Error> {
    let mut data = form_ctx.get_submit_data();

    data["hostname"] = connection_params.hostname.into();
    if let Some(fp) = connection_params.fingerprint {
        data["fingerprint"] = fp.into();
    }

    let data: ScanParams = serde_json::from_value(data.clone())?;

    let params = serde_json::to_value(&data)?;
    let mut result: Remote = proxmox_yew_comp::http_post("/pve/scan", Some(params)).await?;
    result.nodes.insert(
        0,
        PropertyString::new(NodeUrl {
            hostname: data.hostname,
            fingerprint: data.fingerprint,
        }),
    );
    result.nodes.sort_by(|a, b| a.hostname.cmp(&b.hostname));
    Ok(result)
}

impl PdmWizardPageInfo {
    fn update_credentials(form_ctx: &FormContext) {
        let user = form_ctx.read().get_field_text("user");
        let realm = form_ctx.read().get_field_text("realm");
        let password = form_ctx.read().get_field_text("password");

        let user_mode = form_ctx.read().get_field_text("login-mode") == "login";

        let tokenid = form_ctx.read().get_field_text("tokenid");
        let secret = form_ctx.read().get_field_text("secret");

        let (authid, token) =
            if user_mode && !user.is_empty() && !realm.is_empty() && !password.is_empty() {
                (format!("{user}@{realm}").into(), password.into())
            } else if !user_mode && !tokenid.is_empty() && !secret.is_empty() {
                (tokenid.into(), secret.into())
            } else {
                (serde_json::Value::Null, serde_json::Value::Null)
            };

        form_ctx.write().set_field_value("authid", authid);
        form_ctx.write().set_field_value("token", token);
    }

    fn update_server_info(&mut self, ctx: &Context<Self>, server_info: Option<Remote>) {
        let props = ctx.props();
        self.server_info = server_info;
        props.info.page_lock(self.server_info.is_none());
        if let Some(on_server_change) = &props.on_server_change {
            on_server_change.emit(self.server_info.clone());
        }
    }
}

impl Component for PdmWizardPageInfo {
    type Message = Msg;
    type Properties = WizardPageInfo;

    fn create(ctx: &Context<Self>) -> Self {
        let props = ctx.props();

        props.info.page_lock(true);

        let _form_observer = props
            .info
            .form_ctx
            .add_listener(ctx.link().callback(|_| Msg::FormChange));

        Self {
            server_info: None,
            user_mode: true,
            realms: create_realm_list(props),
            _form_observer,
            last_error: None,
            loading: false,
            credentials: None,
            async_pool: AsyncPool::new(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let props = ctx.props();
        match msg {
            Msg::ToggleCreateToken(create_token) => {
                self.user_mode = create_token;
            }
            Msg::FormChange => {
                let form_ctx = &props.info.form_ctx;
                Self::update_credentials(form_ctx);
                let authid = form_ctx.read().get_field_text("authid");
                let token = form_ctx.read().get_field_text("token");
                if !authid.is_empty() && !token.is_empty() {
                    match &self.credentials {
                        Some((old_auth, old_token))
                            if *old_auth == authid && *old_token == token => {}
                        Some(_) | None => {
                            self.credentials = Some((authid, token));
                            self.update_server_info(ctx, None);
                        }
                    }
                } else {
                    self.credentials = None;
                }
            }
            Msg::Connect => {
                let link = ctx.link().clone();
                self.update_server_info(ctx, None);
                let form_ctx = props.info.form_ctx.clone();
                self.loading = true;
                self.last_error = None;

                if let Some(connection_info) = props.connect_info.clone() {
                    self.async_pool.spawn(async move {
                        let result = scan(connection_info, form_ctx).await;
                        link.send_message(Msg::ConnectResult(result));
                    });
                } else {
                    unreachable!("Settings page must have connection info");
                }
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

    fn changed(&mut self, ctx: &Context<Self>, _old_props: &Self::Properties) -> bool {
        self.realms = create_realm_list(ctx.props());
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let input_panel = InputPanel::new()
            .class(FlexFit)
            .padding(4)
            .with_field(tr!("Remote ID"), Field::new().name("id").required(true))
            .with_custom_child(
                RadioButton::new("login")
                    .key("login-mode-login")
                    .name("login-mode")
                    .default(true)
                    .box_label(tr!("Login and create Token"))
                    .on_change(
                        ctx.link()
                            .callback(|value| Msg::ToggleCreateToken(value == "login")),
                    ),
            )
            .with_field(
                tr!("User"),
                Field::new()
                    .name("user")
                    .disabled(!self.user_mode)
                    .required(self.user_mode)
                    .submit(false),
            )
            .with_field(
                tr!("Password"),
                Field::new()
                    .input_type(InputType::Password)
                    .name("password")
                    .disabled(!self.user_mode)
                    .required(self.user_mode)
                    .submit(false),
            )
            .with_field(
                tr!("Realm"),
                Combobox::new()
                    .name("realm")
                    .disabled(!self.user_mode)
                    .required(self.user_mode)
                    .items(self.realms.clone())
                    .submit(false),
            )
            .with_field(
                tr!("API Token Name"),
                Field::new()
                    .name("create-token")
                    .disabled(!self.user_mode)
                    .required(self.user_mode)
                    .submit(self.user_mode)
                    .default("pdm-admin"),
            )
            .with_right_custom_child(Container::new().key("spacer")) //spacer
            .with_right_custom_child(
                RadioButton::new("token")
                    .key("login-mode-token")
                    .name("login-mode")
                    .box_label(tr!("Use existing Token")),
            )
            .with_right_field(
                tr!("Token"),
                Field::new()
                    .name("tokenid")
                    .disabled(self.user_mode)
                    .required(!self.user_mode)
                    .submit(false),
            )
            .with_right_field(
                tr!("Secret"),
                Field::new()
                    .name("secret")
                    .input_type(InputType::Password)
                    .disabled(self.user_mode)
                    .required(!self.user_mode)
                    .submit(false),
            )
            .with_field_and_options(
                pwt::widget::FieldPosition::Left,
                false,
                true,
                tr!(""),
                Field::new().name("token").required(true),
            )
            .with_field_and_options(
                pwt::widget::FieldPosition::Left,
                false,
                true,
                tr!(""),
                Field::new().name("authid").required(true),
            );
        let content = Column::new()
            .class(FlexFit)
            .with_child(input_panel)
            .with_child(
                Row::new()
                    .padding(2)
                    .gap(2)
                    .class(css::AlignItems::Center)
                    .with_optional_child(
                        self.last_error
                            .as_deref()
                            .map(|err| error_message(&err.to_string())),
                    )
                    .with_flex_spacer()
                    .with_optional_child(
                        (self.last_error.is_none() && self.server_info.is_some())
                            .then_some(Container::new().with_child(tr!("Scan OK"))),
                    )
                    .with_child(
                        Button::new("Scan")
                            .disabled(self.credentials.is_none())
                            .onclick(ctx.link().callback(|_| Msg::Connect)),
                    ),
            );
        Mask::new(content)
            .class(FlexFit)
            .visible(self.loading)
            .into()
    }
}

impl From<WizardPageInfo> for VNode {
    fn from(val: WizardPageInfo) -> Self {
        let comp = VComp::new::<PdmWizardPageInfo>(Rc::new(val), None);
        VNode::from(comp)
    }
}
