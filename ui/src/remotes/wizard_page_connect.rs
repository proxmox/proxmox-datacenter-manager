use std::rc::Rc;

use anyhow::Error;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use yew::html::IntoEventCallback;
use yew::virtual_dom::{Key, VComp, VNode};

use pwt::css::{FlexFit, JustifyContent};
use pwt::widget::form::{Field, FormContext, FormContextObserver};
use pwt::widget::{error_message, Button, Column, Container, Dialog, InputPanel, Mask, Row};
use pwt::{prelude::*, AsyncAbortGuard};
use pwt_macros::builder;

use proxmox_yew_comp::{KVGrid, KVGridRow, SchemaValidation, WizardPageRenderInfo};

use pdm_api_types::remotes::{RemoteType, TlsProbeOutcome};
use pdm_api_types::CERT_FINGERPRINT_SHA256_SCHEMA;
use proxmox_acme_api::CertificateInfo;

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

#[derive(PartialEq, Clone, Deserialize, Serialize)]
/// Parameters for connect call.
pub struct ConnectParams {
    pub hostname: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

async fn connect(form_ctx: FormContext, remote_type: RemoteType) -> Result<TlsProbeOutcome, Error> {
    let hostname = normalize_hostname(form_ctx.read().get_field_text("hostname"));
    let fingerprint = get_fingerprint(&form_ctx);
    let pdm_client = crate::pdm_client();
    match remote_type {
        RemoteType::Pve => pdm_client
            .pve_probe_tls(&hostname, fingerprint.as_deref())
            .await
            .map_err(Error::from),
        RemoteType::Pbs => pdm_client
            .pbs_probe_tls(&hostname, fingerprint.as_deref())
            .await
            .map_err(Error::from),
    }
}

pub enum Msg {
    FormChange,
    ConfirmResult(bool), // accept or dismiss
    Connect,
    ConnectResult(Result<TlsProbeOutcome, Error>),
}
pub struct PdmWizardPageConnect {
    _form_observer: FormContextObserver,
    loading: bool,
    scan_result: Option<Result<TlsProbeOutcome, Error>>,
    scan_guard: Option<AsyncAbortGuard>,
    rows: Rc<Vec<KVGridRow>>,
}

impl PdmWizardPageConnect {
    fn create_certificate_confirmation_dialog(&self, ctx: &Context<Self>) -> Option<Dialog> {
        let link = ctx.link();
        let certificate = match &self.scan_result {
            Some(Ok(TlsProbeOutcome::UntrustedCertificate(info))) => info.clone(),
            _ => return None,
        };
        Some(
            Dialog::new(tr!("Connection Certificate"))
                .on_close(link.callback(|_| Msg::ConfirmResult(false)))
                .with_child(
                    Column::new()
                        .padding(2)
                        .gap(2)
                        .class(FlexFit)
                        .with_child(Container::new().with_child(tr!(
                            "The certificate of the remote server is not trusted."
                        )))
                        .with_child(Container::new().with_child(tr!(
                            "Do you want to trust the certificate and save its fingerprint?"
                        )))
                        .with_child(
                            KVGrid::new()
                                .class(FlexFit)
                                .borderless(true)
                                .striped(false)
                                .rows(self.rows.clone())
                                .data(Rc::new(
                                    serde_json::to_value(certificate).unwrap_or_default(),
                                )),
                        )
                        .with_child(
                            Row::new()
                                .gap(2)
                                .class(JustifyContent::Center)
                                .with_child(
                                    Button::new(tr!("Yes"))
                                        .onclick(link.callback(|_| Msg::ConfirmResult(true))),
                                )
                                .with_child(
                                    Button::new(tr!("No"))
                                        .onclick(link.callback(|_| Msg::ConfirmResult(false))),
                                ),
                        ),
                ),
        )
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
        props.info.on_next({
            let link = ctx.link().clone();
            move |_| {
                link.send_message(Msg::Connect);
                false
            }
        });

        Self {
            _form_observer,
            loading: false,
            scan_result: None,
            scan_guard: None,
            rows: Rc::new(rows()),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let props = ctx.props();
        match msg {
            Msg::FormChange => {
                props.info.page_lock(!props.info.form_ctx.read().is_valid());
                props.info.reset_remaining_valid_pages();
                for page in ["nodes", "info"] {
                    if let Some(form_ctx) = props.info.lookup_form_context(&Key::from(page)) {
                        form_ctx.write().reset_form();
                    }
                }
                self.scan_result = None;
            }
            Msg::Connect => {
                self.loading = true;
                props.info.page_lock(true);

                self.scan_guard = Some(AsyncAbortGuard::spawn({
                    let link = ctx.link().clone();
                    let form_ctx = props.info.form_ctx.clone();
                    let remote_type = props.remote_type;

                    async move {
                        let result = connect(form_ctx, remote_type).await;
                        link.send_message(Msg::ConnectResult(result));
                    }
                }));
            }
            Msg::ConnectResult(scan_result) => {
                self.loading = false;
                props.info.page_lock(false);
                self.scan_result = Some(scan_result);
                match &self.scan_result {
                    Some(Ok(TlsProbeOutcome::TrustedCertificate)) => {
                        return <Self as Component>::update(self, ctx, Msg::ConfirmResult(true));
                    }
                    Some(Err(_)) => props.info.page_lock(true),
                    _ => {}
                }
            }
            Msg::ConfirmResult(confirm) => {
                if !confirm {
                    self.scan_result = None;
                    return true;
                }
                if let Some(Ok(result)) = &self.scan_result {
                    let connection = match result {
                        TlsProbeOutcome::TrustedCertificate => None,
                        TlsProbeOutcome::UntrustedCertificate(info) => {
                            props.info.form_ctx.write().set_field_value(
                                "fingerprint",
                                info.fingerprint.clone().unwrap_or_default().into(),
                            );
                            Some(info.clone())
                        }
                    };
                    call_on_connect_change(props, connection);
                    self.scan_result = None;
                    props.info.reset_remaining_valid_pages();
                    props.info.go_to_next_page();
                }
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let error = match &self.scan_result {
            Some(Err(err)) => Some(err),
            _ => None,
        };
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
            .with_optional_child(error.map(|err| error_message(&err.to_string())))
            .with_optional_child(self.create_certificate_confirmation_dialog(ctx));

        Mask::new(content)
            .class(FlexFit)
            .visible(self.loading)
            .into()
    }
}

fn get_fingerprint(form_ctx: &FormContext) -> Option<String> {
    let fingerprint = form_ctx.read().get_field_text("fingerprint");

    if fingerprint.is_empty() {
        None
    } else {
        Some(fingerprint)
    }
}

fn call_on_connect_change(props: &WizardPageConnect, certificate_info: Option<CertificateInfo>) {
    if let Some(on_connect_change) = &props.on_connect_change {
        let fingerprint = get_fingerprint(&props.info.form_ctx);
        on_connect_change.emit(Some(ConnectParams {
            hostname: normalize_hostname(props.info.form_ctx.read().get_field_text("hostname")),
            fingerprint: certificate_info
                .and_then(|cert| cert.fingerprint)
                .or(fingerprint),
        }));
    }
}

fn normalize_hostname(hostname: String) -> String {
    let mut result = hostname;
    if let Some(hostname) = result.strip_prefix("http://") {
        result = hostname.to_string();
    }
    if let Some(hostname) = result.strip_prefix("https://") {
        result = hostname.to_string();
    }
    if let Some(hostname) = result.strip_suffix("/") {
        result = hostname.to_string();
    }
    result
}

fn rows() -> Vec<KVGridRow> {
    let render_date = |_name: &str, value: &Value, _record: &Value| -> Html {
        match value.as_i64() {
            Some(value) => html! {proxmox_yew_comp::utils::render_epoch(value)},
            None => html! {value.to_string()},
        }
    };
    let value = vec![
        KVGridRow::new("fingerprint", tr!("Fingerprint")),
        KVGridRow::new("issuer", tr!("Issuer")),
        KVGridRow::new("subject", tr!("Subject")),
        KVGridRow::new("public-key-type", tr!("Public Key Algorithm")),
        KVGridRow::new("public-key-bits", tr!("Public Key Size")),
        KVGridRow::new("notbefore", tr!("Valid Since")).renderer(render_date),
        KVGridRow::new("notafter", tr!("Expires")).renderer(render_date),
        KVGridRow::new("san", tr!("Subject Alternative Names")).renderer(
            |_name, value, _record| {
                let list: Result<Vec<String>, _> = serde_json::from_value(value.clone());
                match list {
                    Ok(value) => {
                        html! {<pre>{&value.join("\n")}</pre>}
                    }
                    _ => html! {value.to_string()},
                }
            },
        ),
    ];
    value
}

impl Into<VNode> for WizardPageConnect {
    fn into(self) -> VNode {
        let comp = VComp::new::<PdmWizardPageConnect>(Rc::new(self), None);
        VNode::from(comp)
    }
}
