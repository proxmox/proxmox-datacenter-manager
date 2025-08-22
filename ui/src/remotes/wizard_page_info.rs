use std::rc::Rc;

use anyhow::Error;
use html::IntoEventCallback;
use serde::{Deserialize, Serialize};
use yew::virtual_dom::{Key, VComp, VNode};

use proxmox_schema::property_string::PropertyString;
use proxmox_yew_comp::WizardPageRenderInfo;
use pwt::{
    css::{self, FlexFit},
    prelude::*,
    widget::{
        error_message,
        form::{Field, FormContext, FormContextObserver, InputType, RadioButton},
        Column, Container, InputPanel, Mask, Row,
    },
    AsyncPool,
};

use pdm_api_types::remotes::{NodeUrl, Remote};

use pwt_macros::builder;

use super::wizard_page_connect::ConnectParams;
use crate::widget::PveRealmSelector;

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

async fn scan(connection_params: ConnectParams, form_ctx: FormContext) -> Result<Remote, Error> {
    let mut data = form_ctx.get_submit_data();

    data["hostname"] = connection_params.hostname.into();
    if let Some(fp) = connection_params.fingerprint {
        data["fingerprint"] = fp.into();
    }

    let ScanParams {
        hostname,
        authid,
        token,
        fingerprint,
    } = serde_json::from_value(data.clone())?;

    let mut result = crate::pdm_client()
        .pve_scan_remote(&hostname, fingerprint.as_deref(), &authid, &token)
        .await?;

    // try to deduplicate the entered info from the first page with the nodelist here
    // either via the hostname or the fingerprint. if none matches the entered info will
    // be an extra entry in the first position
    let mut found_matching_host = false;
    for node in result.nodes.iter_mut() {
        if node.hostname == hostname {
            if fingerprint.is_none() {
                node.fingerprint = None;
            }
            found_matching_host = true;
            continue;
        }
        if node.fingerprint.as_ref().map(|fp| fp.to_uppercase())
            == fingerprint.as_ref().map(|fp| fp.to_uppercase())
        {
            found_matching_host = true;
            node.hostname = hostname.clone();
            continue;
        }
    }
    if !found_matching_host {
        result.nodes.insert(
            0,
            PropertyString::new(NodeUrl {
                hostname,
                fingerprint: fingerprint.map(|fp| fp.to_uppercase()),
            }),
        );
    }
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

        props.info.on_next({
            let link = ctx.link().clone();
            move |_| {
                link.send_message(Msg::Connect);
                false
            }
        });

        Self {
            server_info: None,
            user_mode: true,
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
                if let Some(form_ctx) = props.info.lookup_form_context(&Key::from("nodes")) {
                    let mut form = form_ctx.write();
                    form.set_field_value("nodes", serde_json::Value::Null);
                    form.reset_form();
                }
                props.info.page_lock(self.credentials.is_none());
            }
            Msg::Connect => {
                if self.server_info.is_some() {
                    props.info.page_lock(false);
                    props.info.go_to_next_page();
                    return true;
                }
                let link = ctx.link().clone();
                self.update_server_info(ctx, None);
                let form_ctx = props.info.form_ctx.clone();
                self.loading = true;
                self.last_error = None;
                props.info.page_lock(true);

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
                props.info.page_lock(false);
                match server_info {
                    Ok(server_info) => {
                        self.update_server_info(ctx, Some(server_info));
                    }
                    Err(err) => {
                        self.last_error = Some(err);
                        props.info.page_lock(true);
                    }
                }

                props.info.reset_remaining_valid_pages();

                if self.last_error.is_none() {
                    props.info.go_to_next_page();
                }
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let (hostname, fingerprint) = match props.connect_info.clone() {
            Some(info) => (info.hostname, info.fingerprint),
            None => (Default::default(), None),
        };
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
                PveRealmSelector::new(hostname, fingerprint)
                    .name("realm")
                    .disabled(!self.user_mode)
                    .required(self.user_mode),
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
                "",
                Field::new().name("token").required(true),
            )
            .with_field_and_options(
                pwt::widget::FieldPosition::Left,
                false,
                true,
                "",
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
