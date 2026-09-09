use std::collections::HashMap;
use std::future::Future;
use std::rc::Rc;

use futures::{FutureExt, StreamExt};
use gloo_timers::future::TimeoutFuture;
use http::StatusCode;
use pdm_api_types::remotes::{REMOTE_ONBOARDING_TIMEOUT_SECS, TlsProbeOutcome};
use proxmox_schema::property_string::PropertyString;
use serde_json::Value;
use yew::virtual_dom::{VComp, VNode};

use pwt::css::{FlexFit, FontStyle, JustifyContent, Overflow};
use pwt::widget::{Button, Column, Container, Dialog, Row, error_message};
use pwt::{AsyncAbortGuard, prelude::*};
use pwt_macros::builder;

use proxmox_yew_comp::{KVGrid, KVGridRow, WizardPageRenderInfo};

use pdm_api_types::{CertificateInfo, remotes::NodeUrl};
use pdm_client::types::Remote;

use super::NodeUrlList;
use super::connection_attempt::{
    ConnectionAttempt, attempt_mask, is_gateway_timeout_client, monotonic_now_millis, retry_row,
};

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

#[derive(Clone, Copy)]
struct EndpointScanDeadline {
    expires_at_millis: f64,
}

impl EndpointScanDeadline {
    fn new(expires_at_millis: f64) -> Self {
        Self { expires_at_millis }
    }

    fn is_expired_at(self, now_millis: f64) -> bool {
        now_millis >= self.expires_at_millis
    }

    fn remaining_millis_at(self, now_millis: f64) -> u32 {
        (self.expires_at_millis - now_millis)
            .clamp(0.0, u32::MAX as f64)
            .floor() as u32
    }
}

#[derive(Debug, Eq, PartialEq)]
enum EndpointScanOutcome {
    Finished,
    DeadlineExpired,
}

#[derive(Debug, Eq, PartialEq)]
enum EndpointCheckDecision {
    Advance,
    AwaitCertificateConfirmation,
    Probe,
}

pub enum Msg {
    Scan,
    ScanResult {
        scan_id: usize,
        index: usize,
        result: Result<TlsProbeOutcome, proxmox_client::Error>,
    },
    ScanFinished(usize),
    ScanDeadlineExpired {
        scan_id: usize,
        endpoint_indices: Vec<usize>,
    },
    ConfirmResult(bool),
    Cancel,
    Retry,
    Tick,
}

pub struct PdmWizardPageNodes {
    endpoint_checks: Vec<EndpointCheck>,
    scan_guard: Option<AsyncAbortGuard>,
    scan_id: usize,
    loading: bool,
    attempt: Option<ConnectionAttempt>,
    certificate_rows: Rc<Vec<KVGridRow>>,
}

fn set_endpoint_result(checks: &mut [EndpointCheck], index: usize, result: ProbeResult) {
    if let Some(check) = checks.get_mut(index) {
        check.result = Some(result);
    }
}

fn endpoint_deadline_error() -> proxmox_client::Error {
    let limit = REMOTE_ONBOARDING_TIMEOUT_SECS;
    proxmox_client::Error::Api(
        StatusCode::GATEWAY_TIMEOUT,
        format!("endpoint checks timed out after their total limit of {limit} seconds"),
    )
}

fn expire_unfinished_endpoint_checks(checks: &mut [EndpointCheck], endpoint_indices: &[usize]) {
    for &index in endpoint_indices {
        if let Some(check) = checks.get_mut(index) {
            if check.result.is_none() {
                check.result = Some(Err(endpoint_deadline_error()));
            }
        }
    }
}

fn all_endpoint_checks_are_trusted(checks: &[EndpointCheck]) -> bool {
    !checks.is_empty()
        && checks.iter().all(|check| {
            matches!(
                check.result.as_ref(),
                Some(Ok(TlsProbeOutcome::TrustedCertificate))
            )
        })
}

fn endpoint_nodes_match(checks: &[EndpointCheck], nodes: &[NodeUrl]) -> bool {
    checks.len() == nodes.len()
        && checks
            .iter()
            .zip(nodes)
            .all(|(check, node)| check.node == *node)
}

fn endpoint_check_decision(checks: &[EndpointCheck], nodes: &[NodeUrl]) -> EndpointCheckDecision {
    if !endpoint_nodes_match(checks, nodes) {
        return EndpointCheckDecision::Probe;
    }

    if all_endpoint_checks_are_trusted(checks) {
        return EndpointCheckDecision::Advance;
    }

    if checks
        .iter()
        .all(|check| check.result.as_ref().is_some_and(Result::is_ok))
    {
        EndpointCheckDecision::AwaitCertificateConfirmation
    } else {
        EndpointCheckDecision::Probe
    }
}

fn can_confirm_endpoint_certificates(checks: &[EndpointCheck], nodes_value: Option<Value>) -> bool {
    parse_endpoint_nodes(nodes_value).is_some_and(|nodes| {
        endpoint_check_decision(checks, &nodes)
            == EndpointCheckDecision::AwaitCertificateConfirmation
    })
}

fn parse_endpoint_nodes(value: Option<Value>) -> Option<Vec<NodeUrl>> {
    let Value::Array(nodes) = value? else {
        return None;
    };
    let nodes = nodes
        .into_iter()
        .map(|node| {
            serde_json::from_value::<PropertyString<NodeUrl>>(node).map(PropertyString::into_inner)
        })
        .collect::<Result<Vec<_>, _>>()
        .ok()?;

    (!nodes.is_empty()).then_some(nodes)
}

async fn run_endpoint_checks<D, N, P, PF, C>(
    nodes: Vec<(usize, NodeUrl)>,
    deadline: EndpointScanDeadline,
    deadline_future: D,
    now: N,
    probe: P,
    mut on_result: C,
) -> EndpointScanOutcome
where
    D: Future<Output = ()>,
    N: Fn() -> f64 + Clone,
    P: Fn(NodeUrl) -> PF + Clone,
    PF: Future<Output = ProbeResult>,
    C: FnMut(usize, ProbeResult),
{
    let checks = futures::stream::iter(nodes.into_iter().map(|(index, node)| {
        let now = now.clone();
        let probe = probe.clone();
        async move {
            if deadline.is_expired_at(now()) {
                return (index, None);
            }

            let result = probe(node).await;
            let result = (!deadline.is_expired_at(now())).then_some(result);
            (index, result)
        }
    }))
    .buffer_unordered(MAX_CONCURRENT_ENDPOINT_CHECKS)
    .fuse();
    let deadline_future = deadline_future.fuse();
    futures::pin_mut!(checks, deadline_future);

    loop {
        futures::select_biased! {
            _ = deadline_future => return EndpointScanOutcome::DeadlineExpired,
            next = checks.next() => match next {
                Some((index, Some(result))) => on_result(index, result),
                Some((_, None)) => return EndpointScanOutcome::DeadlineExpired,
                None => return EndpointScanOutcome::Finished,
            },
        }
    }
}

fn prepare_endpoint_checks(
    checks: &mut Vec<EndpointCheck>,
    nodes: Vec<NodeUrl>,
) -> Vec<(usize, NodeUrl)> {
    if !endpoint_nodes_match(checks, &nodes) {
        *checks = nodes
            .into_iter()
            .map(|node| EndpointCheck { node, result: None })
            .collect();
    }

    checks
        .iter_mut()
        .enumerate()
        .filter_map(|(index, check)| {
            if check.result.as_ref().is_some_and(Result::is_ok) {
                None
            } else {
                check.result = None;
                Some((index, check.node.clone()))
            }
        })
        .collect()
}

impl PdmWizardPageNodes {
    fn abort_scan(&mut self) {
        self.scan_guard = None;
        self.attempt = None;
        self.loading = false;
        self.scan_id = self.scan_id.wrapping_add(1);
    }

    fn start_scan(&mut self, ctx: &Context<Self>, nodes: Vec<(usize, NodeUrl)>) {
        self.abort_scan();
        if nodes.is_empty() {
            return;
        }

        self.loading = true;
        ctx.props().info.page_lock(true);
        let link = ctx.link().clone();
        let attempt = ConnectionAttempt::new(link.callback(|_| Msg::Tick));
        let deadline = EndpointScanDeadline::new(attempt.deadline_millis());
        let timeout_millis = deadline.remaining_millis_at(monotonic_now_millis());
        let deadline_future = TimeoutFuture::new(timeout_millis);
        self.attempt = Some(attempt);
        let scan_id = self.scan_id;
        let endpoint_indices = nodes.iter().map(|(index, _)| *index).collect();
        self.scan_guard = Some(AsyncAbortGuard::spawn(async move {
            let result_link = link.clone();
            let outcome = run_endpoint_checks(
                nodes,
                deadline,
                deadline_future,
                monotonic_now_millis,
                move |node| async move {
                    crate::pdm_client()
                        .pve_probe_tls(&node.hostname, node.fingerprint.as_deref())
                        .await
                },
                move |index, result| {
                    result_link.send_message(Msg::ScanResult {
                        scan_id,
                        index,
                        result,
                    });
                },
            )
            .await;

            match outcome {
                EndpointScanOutcome::Finished => link.send_message(Msg::ScanFinished(scan_id)),
                EndpointScanOutcome::DeadlineExpired => {
                    link.send_message(Msg::ScanDeadlineExpired {
                        scan_id,
                        endpoint_indices,
                    });
                }
            }
        }));
    }

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
            attempt: None,
            certificate_rows: Rc::new(rows()),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let props = ctx.props();
        match msg {
            Msg::Scan => {
                let nodes = props.info.form_ctx.read().get_field_value("nodes");
                let Some(nodes) = parse_endpoint_nodes(nodes) else {
                    return true;
                };

                match endpoint_check_decision(&self.endpoint_checks, &nodes) {
                    EndpointCheckDecision::Advance => props.info.go_to_next_page(),
                    EndpointCheckDecision::AwaitCertificateConfirmation => {}
                    EndpointCheckDecision::Probe => {
                        let nodes = prepare_endpoint_checks(&mut self.endpoint_checks, nodes);
                        self.start_scan(ctx, nodes);
                    }
                }
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

                self.scan_guard = None;
                self.attempt = None;
                self.loading = false;
                props.info.page_lock(!props.info.form_ctx.read().is_valid());

                if all_endpoint_checks_are_trusted(&self.endpoint_checks) {
                    props.info.go_to_next_page();
                }
            }
            Msg::ScanDeadlineExpired {
                scan_id,
                endpoint_indices,
            } => {
                if scan_id != self.scan_id {
                    return false;
                }

                self.abort_scan();
                expire_unfinished_endpoint_checks(&mut self.endpoint_checks, &endpoint_indices);
                props.info.page_lock(!props.info.form_ctx.read().is_valid());

                if all_endpoint_checks_are_trusted(&self.endpoint_checks) {
                    props.info.go_to_next_page();
                }
            }
            Msg::Cancel => {
                self.abort_scan();
                props.info.page_lock(!props.info.form_ctx.read().is_valid());
            }
            Msg::Retry => {
                let nodes = props.info.form_ctx.read().get_field_value("nodes");
                let Some(nodes) = parse_endpoint_nodes(nodes) else {
                    return true;
                };
                let nodes = prepare_endpoint_checks(&mut self.endpoint_checks, nodes);
                self.start_scan(ctx, nodes);
            }
            Msg::Tick => {
                return self.attempt.as_mut().is_some_and(ConnectionAttempt::tick);
            }
            Msg::ConfirmResult(confirm) => {
                if confirm {
                    if !can_confirm_endpoint_certificates(
                        &self.endpoint_checks,
                        props.info.form_ctx.read().get_field_value("nodes"),
                    ) {
                        return true;
                    }

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

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        if old_props.info.visible && !ctx.props().info.visible && self.loading {
            self.abort_scan();
            ctx.props()
                .info
                .page_lock(!ctx.props().info.form_ctx.read().is_valid());
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
                    // The shared deadline also expires endpoints still waiting in the queue, so do
                    // not claim that each one was waited on for the whole limit.
                    let detail = if is_gateway_timeout_client(err) {
                        tr!("timed out")
                    } else {
                        err.to_string()
                    };
                    let hostname = &check.node.hostname;
                    errors.push(error_message(&format!("{hostname} - {detail}")).into());
                }
            }
        }

        let has_errors = !errors.is_empty();
        let can_confirm_certificates = !self.loading
            && can_confirm_endpoint_certificates(
                &self.endpoint_checks,
                ctx.props().info.form_ctx.read().get_field_value("nodes"),
            );

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
                (has_errors && !self.loading && ctx.props().info.form_ctx.read().is_valid())
                    .then(|| retry_row(ctx.link().callback(|_| Msg::Retry))),
            )
            .with_optional_child(
                can_confirm_certificates
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

        attempt_mask(
            content,
            self.attempt.as_ref(),
            &operation,
            ctx.link().callback(|_| Msg::Cancel),
        )
        .class(FlexFit)
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

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::task::{Context, Poll};

    use futures::channel::oneshot;
    use futures::task::noop_waker_ref;
    use http::StatusCode;

    use super::*;

    fn endpoint(hostname: &str) -> EndpointCheck {
        EndpointCheck {
            node: NodeUrl {
                hostname: hostname.to_string(),
                fingerprint: None,
            },
            result: None,
        }
    }

    fn certificate() -> CertificateInfo {
        CertificateInfo {
            filename: String::new(),
            subject: "CN=node-a".to_string(),
            san: Vec::new(),
            issuer: "CN=test".to_string(),
            notbefore: None,
            notafter: None,
            pem: None,
            public_key_type: "RSA".to_string(),
            public_key_bits: Some(2048),
            fingerprint: Some("AA:BB".to_string()),
        }
    }

    #[test]
    fn unchanged_trusted_endpoint_checks_advance_without_probe() {
        let nodes = vec![endpoint("node-a").node, endpoint("node-b").node];
        let mut checks = vec![endpoint("node-a"), endpoint("node-b")];
        checks[0].result = Some(Ok(TlsProbeOutcome::TrustedCertificate));
        checks[1].result = Some(Ok(TlsProbeOutcome::TrustedCertificate));

        assert_eq!(
            endpoint_check_decision(&checks, &nodes),
            EndpointCheckDecision::Advance
        );
    }

    #[test]
    fn certificate_confirmation_requires_complete_matching_inputs() {
        let nodes = Some(serde_json::json!(["node-a", "node-b"]));
        let mut checks = vec![endpoint("node-a"), endpoint("node-b")];
        checks[0].result = Some(Ok(TlsProbeOutcome::UntrustedCertificate(certificate())));

        assert!(!can_confirm_endpoint_certificates(&checks, nodes.clone()));
        checks[1].result = Some(Ok(TlsProbeOutcome::TrustedCertificate));
        assert!(can_confirm_endpoint_certificates(&checks, nodes));
        assert!(!can_confirm_endpoint_certificates(
            &checks,
            Some(serde_json::json!(["node-a", "edited-node"])),
        ));
    }

    #[test]
    fn repeated_scan_retries_errors_but_keeps_successful_results() {
        let mut checks = vec![endpoint("node-a"), endpoint("node-b"), endpoint("node-c")];
        checks[0].result = Some(Ok(TlsProbeOutcome::TrustedCertificate));
        checks[1].result = Some(Ok(TlsProbeOutcome::UntrustedCertificate(certificate())));
        checks[2].result = Some(Err(proxmox_client::Error::Api(
            StatusCode::BAD_GATEWAY,
            "failed".to_string(),
        )));
        let nodes = checks
            .iter()
            .map(|check| check.node.clone())
            .collect::<Vec<_>>();

        assert_eq!(
            endpoint_check_decision(&checks, &nodes),
            EndpointCheckDecision::Probe
        );
        let retry = prepare_endpoint_checks(&mut checks, nodes);
        assert_eq!(retry, vec![(2, endpoint("node-c").node)]);
        assert!(matches!(
            checks[0].result,
            Some(Ok(TlsProbeOutcome::TrustedCertificate))
        ));
        assert!(matches!(
            checks[1].result,
            Some(Ok(TlsProbeOutcome::UntrustedCertificate(_)))
        ));
        assert!(checks[2].result.is_none());
    }

    #[test]
    fn empty_or_invalid_endpoint_input_is_rejected() {
        for value in [
            None,
            Some(serde_json::json!([])),
            Some(serde_json::json!([true])),
        ] {
            assert!(parse_endpoint_nodes(value).is_none());
        }
    }

    #[test]
    fn new_or_changed_endpoints_require_probing() {
        let nodes = vec![endpoint("node-b").node];
        let mut old = endpoint("node-a");
        old.result = Some(Ok(TlsProbeOutcome::TrustedCertificate));

        for mut checks in [vec![], vec![old]] {
            assert_eq!(
                endpoint_check_decision(&checks, &nodes),
                EndpointCheckDecision::Probe
            );
            let work = prepare_endpoint_checks(&mut checks, nodes.clone());
            assert_eq!(work, vec![(0, endpoint("node-b").node)]);
            assert!(checks[0].result.is_none());
        }
    }

    #[test]
    fn six_hanging_endpoints_stop_at_one_page_deadline() {
        let starts = Rc::new(Cell::new(0));
        let (expire, expired) = oneshot::channel();
        let nodes = (0..6)
            .map(|index| (index, endpoint(&format!("node-{index}")).node))
            .collect();
        let deadline = EndpointScanDeadline::new(1_000.0);
        let probe_starts = starts.clone();
        let future = run_endpoint_checks(
            nodes,
            deadline,
            async move {
                expired.await.expect("deadline sender must remain alive");
            },
            || 0.0,
            move |_| {
                let starts = probe_starts.clone();
                async move {
                    starts.set(starts.get() + 1);
                    futures::future::pending::<ProbeResult>().await
                }
            },
            |_, _| unreachable!("hanging probes cannot produce results"),
        );
        futures::pin_mut!(future);

        let mut context = Context::from_waker(noop_waker_ref());
        assert!(matches!(future.as_mut().poll(&mut context), Poll::Pending));
        assert_eq!(starts.get(), MAX_CONCURRENT_ENDPOINT_CHECKS);

        expire
            .send(())
            .expect("deadline receiver must remain alive");
        assert_eq!(
            futures::executor::block_on(future),
            EndpointScanOutcome::DeadlineExpired
        );
        assert_eq!(starts.get(), MAX_CONCURRENT_ENDPOINT_CHECKS);
    }

    #[test]
    fn expired_page_budget_starts_no_queued_endpoint_probes() {
        let starts = Rc::new(Cell::new(0));
        let nodes = (0..6)
            .map(|index| (index, endpoint(&format!("node-{index}")).node))
            .collect();
        let probe_starts = starts.clone();

        let outcome = futures::executor::block_on(run_endpoint_checks(
            nodes,
            EndpointScanDeadline::new(1_000.0),
            futures::future::pending(),
            || 1_000.0,
            move |_| {
                probe_starts.set(probe_starts.get() + 1);
                futures::future::ready(Ok(TlsProbeOutcome::TrustedCertificate))
            },
            |_, _| unreachable!("expired probes cannot produce results"),
        ));

        assert_eq!(outcome, EndpointScanOutcome::DeadlineExpired);
        assert_eq!(starts.get(), 0);
    }

    #[test]
    fn deadline_expiry_and_retry_preserve_confirmed_results() {
        let mut checks = vec![
            endpoint("trusted"),
            endpoint("untrusted"),
            endpoint("failed"),
            endpoint("backend-timeout"),
            endpoint("page-timeout-a"),
            endpoint("page-timeout-b"),
            endpoint("outside-attempt"),
        ];
        checks[0].result = Some(Ok(TlsProbeOutcome::TrustedCertificate));
        checks[1].result = Some(Ok(TlsProbeOutcome::UntrustedCertificate(certificate())));
        checks[2].result = Some(Err(proxmox_client::Error::Api(
            StatusCode::BAD_GATEWAY,
            "failed".to_string(),
        )));
        checks[3].result = Some(Err(proxmox_client::Error::Api(
            StatusCode::GATEWAY_TIMEOUT,
            "backend timed out".to_string(),
        )));
        let nodes = checks.iter().map(|check| check.node.clone()).collect();
        expire_unfinished_endpoint_checks(&mut checks, &[0, 1, 2, 3, 4, 5]);
        assert!(checks[4..6].iter().all(|check| matches!(
            check.result,
            Some(Err(proxmox_client::Error::Api(
                StatusCode::GATEWAY_TIMEOUT,
                _
            )))
        )));

        let retry = prepare_endpoint_checks(&mut checks, nodes);

        assert_eq!(
            retry,
            vec![
                (2, endpoint("failed").node),
                (3, endpoint("backend-timeout").node),
                (4, endpoint("page-timeout-a").node),
                (5, endpoint("page-timeout-b").node),
                (6, endpoint("outside-attempt").node),
            ]
        );
        assert!(matches!(
            checks[0].result,
            Some(Ok(TlsProbeOutcome::TrustedCertificate))
        ));
        assert!(matches!(
            checks[1].result,
            Some(Ok(TlsProbeOutcome::UntrustedCertificate(_)))
        ));
        assert!(checks[2..].iter().all(|check| check.result.is_none()));
    }

    #[test]
    fn retry_after_editing_probes_the_current_endpoints() {
        let mut checks = vec![endpoint("old-a"), endpoint("old-b")];
        checks[0].result = Some(Ok(TlsProbeOutcome::TrustedCertificate));
        checks[1].result = Some(Err(endpoint_deadline_error()));
        let edited = vec![endpoint("old-a").node, endpoint("new-b").node];

        let retry = prepare_endpoint_checks(&mut checks, edited.clone());

        assert_eq!(retry.len(), 2);
        assert!(endpoint_nodes_match(&checks, &edited));
        assert!(checks.iter().all(|check| check.result.is_none()));
    }
}
