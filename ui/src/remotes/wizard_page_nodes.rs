use std::collections::HashMap;
use std::rc::Rc;

use futures::StreamExt;
use pdm_api_types::remotes::TlsProbeOutcome;
use proxmox_schema::property_string::PropertyString;
use serde_json::Value;
use yew::virtual_dom::{VComp, VNode};

use pwt::css::{FlexFit, FontStyle, JustifyContent, Overflow};
use pwt::widget::{Button, Column, Container, Dialog, Mask, Row, error_message};
use pwt::{AsyncAbortGuard, prelude::*};
use pwt_macros::builder;

use proxmox_yew_comp::{KVGrid, KVGridRow, WizardPageRenderInfo};

use pdm_api_types::{CertificateInfo, remotes::NodeUrl};
use pdm_client::types::Remote;

use super::NodeUrlList;

#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct WizardPageNodes {
    info: WizardPageRenderInfo,

    #[builder]
    #[prop_or_default]
    server_info: Option<Remote>,
}

impl WizardPageNodes {
    pub fn new(info: WizardPageRenderInfo) -> Self {
        yew::props!(Self { info })
    }
}

const MAX_CONCURRENT_ENDPOINT_CHECKS: usize = 4;

type ProbeResult = Result<TlsProbeOutcome, proxmox_client::Error>;

struct EndpointCheck {
    node: NodeUrl,
    result: Option<ProbeResult>,
}

pub enum Msg {
    Scan,
    ScanResult {
        scan_id: usize,
        index: usize,
        result: Result<TlsProbeOutcome, proxmox_client::Error>,
    },
    ScanFinished(usize),
    ConfirmResult(bool),
}

pub struct PdmWizardPageNodes {
    endpoint_checks: Vec<EndpointCheck>,
    scan_guard: Option<AsyncAbortGuard>,
    scan_id: usize,
    loading: bool,
    certificate_rows: Rc<Vec<KVGridRow>>,
}

fn set_endpoint_result(checks: &mut [EndpointCheck], index: usize, result: ProbeResult) {
    if let Some(check) = checks.get_mut(index) {
        check.result = Some(result);
    }
}

impl PdmWizardPageNodes {
    fn create_certificate_confirmation_dialog(
        &self,
        ctx: &Context<Self>,
        certificates: Vec<(&String, &CertificateInfo)>,
    ) -> Dialog {
        let link = ctx.link();
        Dialog::new(tr!("Connection Certificate"))
            .on_close(link.callback(|_| Msg::ConfirmResult(false)))
            .with_child(
                Column::new()
                    .padding(2)
                    .gap(2)
                    .class(FlexFit)
                    .with_child(Container::new().with_child(tr!(
                        "The following certificates of remote servers are not trusted."
                    )))
                    .with_child(Container::new().with_child(tr!(
                        "Do you want to trust them by saving their fingerprint?"
                    )))
                    .with_child(
                        Column::new()
                            .max_height(400)
                            .gap(1)
                            .padding(2)
                            .class(Overflow::Auto)
                            .children(certificates.into_iter().map(|(hostname, certificate)| {
                                Column::new()
                                    .with_child(
                                        Container::new().class(FontStyle::TitleSmall).with_child(
                                            format!("{}: {hostname}", tr!("Server Address")),
                                        ),
                                    )
                                    .with_child(
                                        KVGrid::new()
                                            .class(FlexFit)
                                            .class("pdm-kvgrid-wrap")
                                            .borderless(true)
                                            .striped(false)
                                            .rows(self.certificate_rows.clone())
                                            .data(Rc::new(
                                                serde_json::to_value(certificate)
                                                    .unwrap_or_default(),
                                            )),
                                    )
                                    .into()
                            })),
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
            )
    }
}

impl Component for PdmWizardPageNodes {
    type Message = Msg;
    type Properties = WizardPageNodes;

    fn create(_ctx: &Context<Self>) -> Self {
        _ctx.props().info.on_next({
            let link = _ctx.link().clone();
            move |_| {
                link.send_message(Msg::Scan);
                false
            }
        });
        Self {
            endpoint_checks: Vec::new(),
            scan_guard: None,
            scan_id: 0,
            loading: false,
            certificate_rows: Rc::new(rows()),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let props = ctx.props();
        match msg {
            Msg::Scan => {
                let nodes = props.info.form_ctx.read().get_field_value("nodes");
                let Some(Value::Array(nodes)) = nodes else {
                    return true;
                };
                let nodes: Vec<_> = nodes
                    .into_iter()
                    .filter_map(|node| {
                        serde_json::from_value::<PropertyString<NodeUrl>>(node)
                            .ok()
                            .map(PropertyString::into_inner)
                    })
                    .collect();
                if nodes.is_empty() {
                    return true;
                }

                self.loading = true;
                self.scan_id = self.scan_id.wrapping_add(1);
                self.endpoint_checks = nodes
                    .iter()
                    .cloned()
                    .map(|node| EndpointCheck { node, result: None })
                    .collect();

                let link = ctx.link().clone();
                let scan_id = self.scan_id;
                self.scan_guard = Some(AsyncAbortGuard::spawn(async move {
                    let checks = futures::stream::iter(nodes.into_iter().enumerate().map(
                        |(index, node)| async move {
                            let result = crate::pdm_client()
                                .pve_probe_tls(&node.hostname, node.fingerprint.as_deref())
                                .await;
                            (index, result)
                        },
                    ))
                    .buffer_unordered(MAX_CONCURRENT_ENDPOINT_CHECKS);
                    futures::pin_mut!(checks);

                    while let Some((index, result)) = checks.next().await {
                        link.send_message(Msg::ScanResult {
                            scan_id,
                            index,
                            result,
                        });
                    }
                    link.send_message(Msg::ScanFinished(scan_id));
                }));
            }
            Msg::ScanResult {
                scan_id,
                index,
                result,
            } => {
                if scan_id == self.scan_id {
                    set_endpoint_result(&mut self.endpoint_checks, index, result);
                }
            }
            Msg::ScanFinished(scan_id) => {
                if scan_id != self.scan_id {
                    return false;
                }

                self.loading = false;
                let success = !self.endpoint_checks.is_empty()
                    && self.endpoint_checks.iter().all(|check| {
                        matches!(
                            check.result.as_ref(),
                            Some(Ok(TlsProbeOutcome::TrustedCertificate))
                        )
                    });

                if success {
                    props.info.go_to_next_page();
                }
            }
            Msg::ConfirmResult(confirm) => {
                if confirm {
                    // update connect information with gathered certificate information
                    // and navigate to next page
                    let mut map = HashMap::new();
                    for check in self.endpoint_checks.drain(..) {
                        if let Some(Ok(TlsProbeOutcome::UntrustedCertificate(cert))) = check.result
                        {
                            if let Some(fp) = cert.fingerprint {
                                map.insert(check.node.hostname, fp);
                            }
                        }
                    }

                    let mut form = props.info.form_ctx.write();
                    let value = form
                        .get_field_value("nodes")
                        .unwrap_or(Value::Array(Vec::new()));

                    let value = match serde_json::from_value::<Vec<PropertyString<NodeUrl>>>(value)
                    {
                        Ok(mut nodes) => {
                            for node in nodes.iter_mut() {
                                if node.fingerprint.is_none() && map.contains_key(&node.hostname) {
                                    node.fingerprint =
                                        Some(map.get(&node.hostname).unwrap().to_uppercase());
                                }
                            }
                            // this should never fail
                            serde_json::to_value(nodes).unwrap()
                        }
                        Err(_) => {
                            // data from field is wrong, this should not happen
                            unreachable!("internal data in node field is wrong");
                        }
                    };

                    form.set_field_value("nodes", value);
                    drop(form);
                    props.info.go_to_next_page();
                } else {
                    self.endpoint_checks.clear();
                }
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let nodes = ctx
            .props()
            .server_info
            .as_ref()
            .map(|info| info.nodes.clone())
            .unwrap_or_default();

        let mut errors = Vec::new();
        let mut certificates = Vec::new();

        for check in &self.endpoint_checks {
            match &check.result {
                Some(Ok(TlsProbeOutcome::UntrustedCertificate(cert))) => {
                    certificates.push((&check.node.hostname, cert));
                }
                Some(Ok(_)) | None => {}
                Some(Err(err)) => {
                    errors.push(error_message(&format!("{} - {err}", check.node.hostname)).into());
                }
            }
        }

        let has_errors = !errors.is_empty();

        let content = Container::new()
            .class(FlexFit)
            .padding(4)
            .with_child(Container::new().padding(4).with_child(tr!(
                "Define a set of addresses that Proxmox Datacenter Manager can use to reach the \
                cluster or single node. Fingerprints are only required for untrusted certificates."
            )))
            .with_child(
                NodeUrlList::new()
                    .max_height(400)
                    .default(nodes)
                    .name("nodes")
                    .key("nodes")
                    .required(true),
            )
            .with_optional_child((has_errors).then_some(Column::new().children(errors)))
            .with_optional_child(
                (!self.loading && !has_errors && !certificates.is_empty())
                    .then_some(self.create_certificate_confirmation_dialog(ctx, certificates)),
            );
        let completed = self
            .endpoint_checks
            .iter()
            .filter(|check| check.result.is_some())
            .count();
        let total = self.endpoint_checks.len();
        // Up to four endpoints are checked at once, so report progress rather than a position.
        let operation = tr!(
            "Checked {0} of {n} endpoint" | "Checked {0} of {n} endpoints" % total,
            completed
        );

        Mask::new(content)
            .text(operation)
            .visible(self.loading)
            .into()
    }
}

fn rows() -> Vec<KVGridRow> {
    vec![
        KVGridRow::new("fingerprint", tr!("Fingerprint")),
        KVGridRow::new("issuer", tr!("Issuer")),
        KVGridRow::new("subject", tr!("Subject")),
    ]
}

impl From<WizardPageNodes> for VNode {
    fn from(val: WizardPageNodes) -> Self {
        let comp = VComp::new::<PdmWizardPageNodes>(Rc::new(val), None);
        VNode::from(comp)
    }
}
