use std::collections::HashMap;
use std::rc::Rc;

use pdm_api_types::remotes::TlsProbeOutcome;
use proxmox_schema::property_string::PropertyString;
use serde_json::Value;
use yew::virtual_dom::{VComp, VNode};

use pwt::css::{FlexFit, FontStyle, JustifyContent, Overflow};
use pwt::widget::{error_message, Button, Column, Container, Dialog, Mask, Row};
use pwt::{prelude::*, AsyncAbortGuard};
use pwt_macros::builder;

use proxmox_yew_comp::{KVGrid, KVGridRow, WizardPageRenderInfo};

use pdm_api_types::{remotes::NodeUrl, CertificateInfo};
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

pub enum Msg {
    Scan,
    ScanResult(Vec<(String, Result<TlsProbeOutcome, proxmox_client::Error>)>),
    ConfirmResult(bool),
}

pub struct PdmWizardPageNodes {
    scan_results: Vec<(String, Result<TlsProbeOutcome, proxmox_client::Error>)>,
    scan_guard: Option<AsyncAbortGuard>,
    loading: bool,
    certificate_rows: Rc<Vec<KVGridRow>>,
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
            scan_results: Vec::new(),
            scan_guard: None,
            loading: false,
            certificate_rows: Rc::new(rows()),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let props = ctx.props();
        match msg {
            Msg::Scan => {
                self.loading = true;
                let link = ctx.link().clone();
                let nodes = props.info.form_ctx.read().get_field_value("nodes");
                let Some(Value::Array(nodes)) = nodes else {
                    return true;
                };
                self.scan_guard = Some(AsyncAbortGuard::spawn(async move {
                    let futures = nodes.into_iter().filter_map(|node| {
                        let node = match serde_json::from_value::<PropertyString<NodeUrl>>(node) {
                            Ok(node) => node.into_inner(),
                            Err(_) => return None,
                        };

                        let future = async move {
                            let res = crate::pdm_client()
                                .pve_probe_tls(&node.hostname, node.fingerprint.as_deref())
                                .await;
                            (node.hostname, res)
                        };
                        Some(future)
                    });

                    let res = futures::future::join_all(futures).await;
                    link.send_message(Msg::ScanResult(res));
                }));
            }
            Msg::ScanResult(scan_results) => {
                self.loading = false;
                self.scan_results = scan_results;
                let mut success = true;
                for (_hostname, result) in &self.scan_results {
                    match result {
                        Ok(TlsProbeOutcome::TrustedCertificate) => {}
                        _ => success = false,
                    }
                }

                if success {
                    props.info.go_to_next_page();
                }
            }
            Msg::ConfirmResult(confirm) => {
                if confirm {
                    // update connect information with gathered certificate information
                    // and navigate to next page
                    let mut map = HashMap::new();
                    for (hostname, res) in self.scan_results.drain(..) {
                        if let Ok(TlsProbeOutcome::UntrustedCertificate(cert)) = res {
                            if let Some(fp) = cert.fingerprint {
                                map.insert(hostname, fp);
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
                    self.scan_results.clear();
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

        for (hostname, result) in &self.scan_results {
            match result {
                Ok(TlsProbeOutcome::UntrustedCertificate(cert)) => {
                    certificates.push((hostname, cert));
                }
                Ok(_) => {}
                Err(err) => {
                    errors.push(error_message(&format!("{hostname} - {err}")).into());
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
                (!has_errors && !certificates.is_empty())
                    .then_some(self.create_certificate_confirmation_dialog(ctx, certificates)),
            );
        Mask::new(content).visible(self.loading).into()
    }
}

fn rows() -> Vec<KVGridRow> {
    vec![
        KVGridRow::new("fingerprint", tr!("Fingerprint")),
        KVGridRow::new("issuer", tr!("Issuer")),
        KVGridRow::new("subject", tr!("Subject")),
    ]
}

impl Into<VNode> for WizardPageNodes {
    fn into(self) -> VNode {
        let comp = VComp::new::<PdmWizardPageNodes>(Rc::new(self), None);
        VNode::from(comp)
    }
}
