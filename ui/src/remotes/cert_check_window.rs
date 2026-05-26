//! Re-check a remote's node TLS certificates and offer to update a stored fingerprint after a
//! rotation. Probes ignore the pin; per-row Accept is a frontend mark, the dialog-wide Apply
//! commits every staged pin change in one batch so a single click can never silently rewrite a
//! server-side pin. The first outstanding row auto-expands; staging an action advances to the
//! next, keeping a 40-node remote a guided list rather than a wall of grids.

use std::collections::BTreeMap;
use std::rc::Rc;

use anyhow::Error;
use serde_json::{json, Value};
use yew::html::IntoEventCallback;
use yew::virtual_dom::{Key, VComp, VNode};
use yew::{Callback, Component, Context, Properties};

use proxmox_acme_api::CertificateInfo;
use proxmox_schema::property_string::PropertyString;
use proxmox_yew_comp::percent_encoding::percent_encode_component;
use proxmox_yew_comp::{KVGrid, KVGridRow, Status};
use pwt::css::{AlignItems, Flex, FlexFit, FontStyle, JustifyContent};
use pwt::prelude::*;
use pwt::props::ExtractPrimaryKey;
use pwt::state::{KeyedSlabTree, TreeStore};
use pwt::widget::data_table::{
    DataTable, DataTableCellRenderArgs, DataTableColumn, DataTableHeader,
};
use pwt::widget::{ActionIcon, Button, Column, Container, Dialog, Fa, Row, Toolbar, Tooltip};
use pwt::AsyncPool;
use pwt_macros::builder;

use pdm_api_types::remotes::{NodeUrl, Remote, TlsProbeOutcome};

/// Per-node outcome relative to the stored fingerprint.
#[derive(Clone, PartialEq)]
enum NodeStatus {
    Loading,
    /// Presented cert matches the pin, or (no pin) is system-trusted.
    Matches,
    /// Presented cert is system-trusted; the stored pin may be stale.
    SystemTrusted,
    /// Presented cert's fingerprint differs from the pin.
    Changed(CertificateInfo),
    Failed(String),
}

/// Pending pin change, frontend-only until the dialog-wide Apply commits it.
#[derive(Clone, PartialEq)]
enum Staged {
    /// Adopt the presented fingerprint as the new pin.
    Accept(String),
    /// Clear the pin to rely on the system trust store.
    Clear,
}

impl NodeStatus {
    fn icon_and_token(&self) -> (Fa, String) {
        match self {
            NodeStatus::Loading => (Fa::new("circle-o-notch").class("fa-spin"), tr!("Checking")),
            NodeStatus::Matches => (Fa::from(Status::Success), tr!("Matches")),
            NodeStatus::SystemTrusted => (Fa::from(Status::Success), tr!("Trusted")),
            NodeStatus::Changed(_) => (Fa::from(Status::Warning), tr!("Changed")),
            NodeStatus::Failed(_) => (Fa::from(Status::Error), tr!("Error")),
        }
    }

    fn description(&self) -> String {
        match self {
            NodeStatus::Loading => tr!("Checking certificate..."),
            NodeStatus::Matches => tr!("Certificate matches the stored fingerprint."),
            NodeStatus::SystemTrusted => {
                tr!("Certificate is trusted by the system certificate store.")
            }
            NodeStatus::Changed(_) => tr!("The certificate has changed since it was last trusted."),
            NodeStatus::Failed(err) => tr!("Could not check certificate: {0}", err.clone()),
        }
    }
}

/// Tree row: a node, or a node's expandable certificate detail. The `staged` slot on `Node`
/// mirrors `PdmRemoteCertCheck::staged` so the column renders stay pure; kept in sync by
/// [`refresh_tree_meta`].
#[derive(Clone, PartialEq)]
enum CertTreeEntry {
    Root,
    Node {
        hostname: String,
        status: NodeStatus,
        staged: Option<Staged>,
    },
    Detail {
        hostname: String,
        pinned: Option<String>,
        info: Option<CertificateInfo>,
        /// System-trusted but a stale pin is still set; the detail then only explains the option.
        system_trusted_pinned: bool,
    },
}

impl ExtractPrimaryKey for CertTreeEntry {
    fn extract_key(&self) -> Key {
        match self {
            CertTreeEntry::Root => Key::from("__root__"),
            CertTreeEntry::Node { hostname, .. } => Key::from(hostname.clone()),
            // Distinct prefix so a Detail's key never collides with the Node it sits under.
            CertTreeEntry::Detail { hostname, .. } => Key::from(format!("detail/{hostname}")),
        }
    }
}

#[derive(PartialEq, Properties)]
#[builder]
pub struct RemoteCertCheck {
    pub remote: Remote,

    #[prop_or_default]
    #[builder_cb(IntoEventCallback, into_event_callback, ())]
    pub on_close: Option<Callback<()>>,
}

impl RemoteCertCheck {
    pub fn new(remote: Remote) -> Self {
        yew::props!(Self { remote })
    }
}

impl From<RemoteCertCheck> for VNode {
    fn from(val: RemoteCertCheck) -> Self {
        VNode::from(VComp::new::<PdmRemoteCertCheck>(Rc::new(val), None))
    }
}

enum Msg {
    ProbeResult(String, Result<TlsProbeOutcome, Error>),
    StageAccept(String, String),
    StageClear(String),
    Revert(String),
    Apply,
    ApplyResult(Result<(), Error>),
    RecheckAll,
}

struct PdmRemoteCertCheck {
    store: TreeStore<CertTreeEntry>,
    columns: Rc<Vec<DataTableHeader<CertTreeEntry>>>,
    // Committed pins (server view). Props are not refreshed while the dialog is open, so probes
    // and `spawn_apply` must compare/rebuild against this, not `ctx.props().remote`.
    pins: BTreeMap<String, Option<String>>,
    staged: BTreeMap<String, Staged>,
    /// Last host this dialog auto-expanded, so refresh_tree_meta only collapses its own pick.
    last_auto_expanded: Option<String>,
    apply_error: Option<String>,
    applying: bool,
    _async_pool: AsyncPool,
}

impl PdmRemoteCertCheck {
    fn pinned_fingerprint(&self, hostname: &str) -> Option<String> {
        self.pins.get(hostname).cloned().flatten()
    }

    fn spawn_probes(&self, ctx: &Context<Self>) {
        let remote = ctx.props().remote.clone();
        for node in &remote.nodes {
            self.spawn_probe(ctx, node.hostname.clone());
        }
    }

    fn spawn_probe(&self, ctx: &Context<Self>, hostname: String) {
        let remote_id = ctx.props().remote.id.clone();
        self._async_pool
            .send_future(ctx.link().clone(), async move {
                let res = crate::pdm_client()
                    .remote_probe_certificate(&remote_id, &hostname)
                    .await
                    .map_err(Error::from);
                Msg::ProbeResult(hostname, res)
            });
    }

    /// Submit every staged change as one update_remote call: rebuild the whole `nodes` vector
    /// applying each node's effective pin (staged value if staged, else the committed `pins`).
    fn spawn_apply(&self, ctx: &Context<Self>) {
        let remote_id = ctx.props().remote.id.clone();
        let staged = self.staged.clone();
        let pins = self.pins.clone();
        let nodes: Vec<PropertyString<NodeUrl>> = ctx
            .props()
            .remote
            .nodes
            .iter()
            .map(|ps| {
                let mut url: NodeUrl = (**ps).clone();
                url.fingerprint = match staged.get(&url.hostname) {
                    Some(Staged::Accept(fp)) => Some(fp.clone()),
                    Some(Staged::Clear) => None,
                    None => pins.get(&url.hostname).cloned().flatten(),
                };
                PropertyString::new(url)
            })
            .collect();

        self._async_pool
            .send_future(ctx.link().clone(), async move {
                let res = async {
                    let url = format!("/remotes/remote/{}", percent_encode_component(&remote_id));
                    let body = json!({ "nodes": serde_json::to_value(nodes)? });
                    proxmox_yew_comp::http_put::<Value>(&url, Some(body)).await?;
                    Ok::<(), Error>(())
                }
                .await;
                Msg::ApplyResult(res)
            });
    }

    /// Replace one row's status and rebuild its detail child. Auto-expansion is driven by
    /// [`Self::refresh_tree_meta`] so the workflow advances in one pass.
    fn apply_status(&self, hostname: &str, status: NodeStatus) {
        let pinned = self.pinned_fingerprint(hostname);
        let staged = self.staged.get(hostname).cloned();
        let detail = match &status {
            NodeStatus::Changed(info) => Some(CertTreeEntry::Detail {
                hostname: hostname.to_string(),
                pinned: pinned.clone(),
                info: Some(info.clone()),
                system_trusted_pinned: false,
            }),
            NodeStatus::SystemTrusted if pinned.is_some() => Some(CertTreeEntry::Detail {
                hostname: hostname.to_string(),
                pinned: pinned.clone(),
                info: None,
                system_trusted_pinned: true,
            }),
            _ => None,
        };

        let mut write = self.store.write();
        let Some(mut node) = write.lookup_node_mut(&Key::from(hostname.to_string())) else {
            return;
        };
        *node.record_mut() = CertTreeEntry::Node {
            hostname: hostname.to_string(),
            status,
            staged,
        };
        node.remove_all_children();
        if let Some(detail) = detail {
            node.append(detail);
        }
    }

    /// Sync `self.staged` into every row's slot and expand the next outstanding host (first node in
    /// props order that is Changed or SystemTrusted-with-pin and not staged), advancing on each
    /// accept/revert. Only touches our own previous pick, so manual expansions stick.
    fn refresh_tree_meta(&mut self, ctx: &Context<Self>) {
        // Props order keeps "next" deterministic across the parallel probe completion order.
        let mut next: Option<String> = None;
        {
            let read = self.store.read();
            if let Some(root) = read.lookup_node(&Key::from("__root__")) {
                for node in ctx.props().remote.nodes.iter() {
                    let host = node.hostname.clone();
                    if self.staged.contains_key(&host) {
                        continue;
                    }
                    let needs = root.children().any(|c| {
                        matches!(c.record(),
                        CertTreeEntry::Node {
                            hostname: h,
                            status: NodeStatus::Changed(_) | NodeStatus::SystemTrusted,
                            ..
                        } if h == &host)
                    });
                    if needs {
                        next = Some(host);
                        break;
                    }
                }
            }
        }

        let previous = self.last_auto_expanded.clone();
        {
            let mut write = self.store.write();
            let Some(mut root) = write.lookup_node_mut(&Key::from("__root__")) else {
                return;
            };
            for mut child in root.children_mut() {
                let (hostname, status) = match child.record() {
                    CertTreeEntry::Node {
                        hostname, status, ..
                    } => (hostname.clone(), status.clone()),
                    _ => continue,
                };
                let staged = self.staged.get(&hostname).cloned();
                *child.record_mut() = CertTreeEntry::Node {
                    hostname: hostname.clone(),
                    status,
                    staged,
                };
                // Touch expansion only when our pick is actually advancing, so a manual
                // collapse of the currently-picked row sticks across unrelated probe events.
                if previous.as_deref() != next.as_deref() {
                    if next.as_deref() == Some(hostname.as_str()) {
                        child.set_expanded(true);
                    } else if previous.as_deref() == Some(hostname.as_str()) {
                        child.set_expanded(false);
                    }
                }
            }
        }
        self.last_auto_expanded = next;
    }
}

impl Component for PdmRemoteCertCheck {
    type Message = Msg;
    type Properties = RemoteCertCheck;

    fn create(ctx: &Context<Self>) -> Self {
        let nodes = &ctx.props().remote.nodes;
        let pins = nodes
            .iter()
            .map(|n| (n.hostname.clone(), n.fingerprint.clone()))
            .collect();

        // Hide root so nodes are top-level; seed Loading rows so the table renders pre-probe.
        let store = TreeStore::new().view_root(false);
        let mut tree = KeyedSlabTree::new();
        tree.set_root(CertTreeEntry::Root).set_expanded(true);
        for node in nodes.iter() {
            tree.root_mut().unwrap().append(CertTreeEntry::Node {
                hostname: node.hostname.clone(),
                status: NodeStatus::Loading,
                staged: None,
            });
        }
        store.write().update_root_tree(tree);

        let this = Self {
            columns: columns(ctx.link().clone(), store.clone(), Rc::new(cert_rows())),
            store,
            pins,
            staged: BTreeMap::new(),
            last_auto_expanded: None,
            apply_error: None,
            applying: false,
            _async_pool: AsyncPool::new(),
        };
        this.spawn_probes(ctx);
        this
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        // While applying, ignore staging/recheck clicks; ProbeResult/ApplyResult still flow.
        if self.applying && !matches!(msg, Msg::ProbeResult(..) | Msg::ApplyResult(_)) {
            return false;
        }
        match msg {
            Msg::ProbeResult(hostname, Ok(outcome)) => {
                let pinned = self.pinned_fingerprint(&hostname);
                let new = match outcome {
                    TlsProbeOutcome::TrustedCertificate => match &pinned {
                        // System-trusted but a pin is set: connectivity fine, pin may be stale.
                        Some(_) => NodeStatus::SystemTrusted,
                        None => NodeStatus::Matches,
                    },
                    TlsProbeOutcome::UntrustedCertificate(info) => {
                        let presented = info.fingerprint.clone();
                        match (&pinned, &presented) {
                            (Some(p), Some(c)) if normalize_fp(p) == normalize_fp(c) => {
                                NodeStatus::Matches
                            }
                            _ => NodeStatus::Changed(info),
                        }
                    }
                };
                self.apply_status(&hostname, new);
                self.refresh_tree_meta(ctx);
            }
            Msg::ProbeResult(hostname, Err(err)) => {
                self.apply_status(&hostname, NodeStatus::Failed(err.to_string()));
                self.refresh_tree_meta(ctx);
            }
            Msg::StageAccept(hostname, fp) => {
                self.apply_error = None;
                self.staged.insert(hostname, Staged::Accept(fp));
                self.refresh_tree_meta(ctx);
            }
            Msg::StageClear(hostname) => {
                self.apply_error = None;
                self.staged.insert(hostname, Staged::Clear);
                self.refresh_tree_meta(ctx);
            }
            Msg::Revert(hostname) => {
                self.apply_error = None;
                self.staged.remove(&hostname);
                self.refresh_tree_meta(ctx);
            }
            Msg::Apply => {
                if self.staged.is_empty() || self.applying {
                    return false;
                }
                self.apply_error = None;
                self.applying = true;
                self.spawn_apply(ctx);
            }
            Msg::ApplyResult(Ok(())) => {
                // Commit staged into pins and re-probe so the shown state matches reality
                // (a fresh fingerprint becomes Matches, a cleared pin tracks the system store).
                self.applying = false;
                for (host, action) in std::mem::take(&mut self.staged) {
                    let new = match action {
                        Staged::Accept(fp) => Some(fp),
                        Staged::Clear => None,
                    };
                    self.pins.insert(host.clone(), new);
                    self.apply_status(&host, NodeStatus::Loading);
                    self.spawn_probe(ctx, host);
                }
                self.refresh_tree_meta(ctx);
            }
            Msg::ApplyResult(Err(err)) => {
                self.applying = false;
                self.apply_error = Some(err.to_string());
            }
            Msg::RecheckAll => {
                self.apply_error = None;
                let hosts: Vec<String> = ctx
                    .props()
                    .remote
                    .nodes
                    .iter()
                    .map(|n| n.hostname.clone())
                    .collect();
                for hostname in hosts {
                    self.apply_status(&hostname, NodeStatus::Loading);
                    self.spawn_probe(ctx, hostname);
                }
                self.refresh_tree_meta(ctx);
            }
        }
        // True: the summary header and the Apply button read self.staged / self.applying.
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();

        // Walk live so the header tracks incremental probes; outstanding = Changed or
        // SystemTrusted-with-pin (every row that still offers an action). Track issuers from
        // every row that has a CertificateInfo, to surface a fleet-wide "same issuer" hint once
        // probes have completed and the issuers agree across at least two nodes.
        let mut total = 0usize;
        let mut outstanding = 0usize;
        let mut loading = false;
        let mut issuers: Vec<String> = Vec::new();
        if let Some(root) = self.store.read().root() {
            root.visit(&mut |node| {
                if let CertTreeEntry::Node { status, .. } = node.record() {
                    total += 1;
                    match status {
                        NodeStatus::Changed(info) => {
                            outstanding += 1;
                            issuers.push(info.issuer.clone());
                        }
                        NodeStatus::SystemTrusted => outstanding += 1,
                        NodeStatus::Loading => loading = true,
                        _ => {}
                    }
                }
            });
        }
        let staged_count = self.staged.len();
        // Suppress while any probe is still in flight to avoid flipping in when the last lands;
        // require at least two known certs and agreement on the issuer string.
        let fleet_issuer =
            if !loading && issuers.len() >= 2 && issuers.iter().all(|i| i == &issuers[0]) {
                Some(issuers.swap_remove(0))
            } else {
                None
            };

        let link = ctx.link().clone();
        let summary = Toolbar::new()
            .border_bottom(true)
            .with_child(Container::new().padding_start(2).with_child(tr!(
                "{0} of {1} nodes need review",
                outstanding,
                total
            )))
            .with_flex_spacer()
            .with_child(
                Button::refresh(loading || self.applying)
                    .on_activate(move |_| link.send_message(Msg::RecheckAll)),
            );

        let mut body = Column::new().class(FlexFit);
        body.add_child(summary);
        if let Some(iss) = fleet_issuer {
            // <p> so a long issuer DN wraps with the dialog instead of widening it.
            body.add_child(
                Container::from_tag("p")
                    .padding(2)
                    .with_child(tr!("All certificates are issued by \"{0}\".", iss)),
            );
        }
        if total == 0 {
            body.add_child(
                Container::new()
                    .padding(2)
                    .with_child(tr!("No nodes are configured for this remote.")),
            );
        } else {
            // <p> wraps with the dialog; translations vary widely in resulting text width.
            body.add_child(Container::from_tag("p").padding(2).with_child(tr!(
                "Compare each new SHA-256 fingerprint against the one shown under 'Certificates' \
                 on the remote node's own web interface, accessed through an independently \
                 trusted channel, before accepting it."
            )));
            body.add_child(
                DataTable::new(Rc::clone(&self.columns), self.store.clone()).class(FlexFit),
            );
        }
        if let Some(err) = &self.apply_error {
            body.add_child(pwt::widget::error_message(&tr!(
                "Could not apply changes: {0}",
                err.clone()
            )));
        }

        // Apply commits every staged change in one server call; disabled when idle or in flight.
        let link_apply = ctx.link().clone();
        let apply_label = if staged_count == 0 {
            tr!("Apply Pending")
        } else {
            tr!("Apply Pending ({0})", staged_count)
        };
        let footer = Toolbar::new()
            .border_top(true)
            .with_flex_spacer()
            .with_child(
                Button::new(apply_label)
                    .icon_class("fa fa-play")
                    .disabled(staged_count == 0 || self.applying)
                    .on_activate(move |_| link_apply.send_message(Msg::Apply)),
            );
        body.add_child(footer);

        Dialog::new(tr!("Certificate Check - {0}", props.remote.id.as_str()))
            .min_width(960)
            .min_height(480)
            .max_height("80vh")
            .resizable(true)
            .on_close(props.on_close.clone())
            .with_child(body)
            .into()
    }
}

/// Render the certificate detail (KVGrid + stored-fingerprint line + safety paragraphs).
fn render_detail(
    cert_rows: &Rc<Vec<KVGridRow>>,
    pinned: &Option<String>,
    info: &Option<CertificateInfo>,
    system_trusted_pinned: bool,
) -> Html {
    // The tree column wraps the cell content in a horizontal Row [indent | expander | content];
    // claim the remaining row width via flex: 1 so the inner KVGrid fills the colspan=3 cell.
    let mut col = Column::new().padding(2).gap(2).class(Flex::Fill);

    if system_trusted_pinned {
        col.add_child(Container::new().with_child(tr!(
            "A fingerprint is still pinned. You can clear it to rely on the system \
             trust store instead. Note that the trusted certificate may differ from \
             the one you pinned, so clearing will trust any certificate the system \
             store accepts for this host."
        )));
        return col.into();
    }

    let Some(info) = info else {
        return col.into();
    };
    let new_fp = info.fingerprint.clone().unwrap_or_default();

    col.add_child(
        Row::new()
            .gap(1)
            .class(AlignItems::Baseline)
            .with_child(tr!("Stored fingerprint:"))
            .with_child(html! {
                <code>{ pinned.clone().unwrap_or_else(|| tr!("none")) }</code>
            }),
    );
    col.add_child(
        Container::new()
            .class(FontStyle::TitleSmall)
            .with_child(tr!("Newly probed certificate:")),
    );
    col.add_child(
        KVGrid::new()
            .class("pdm-kvgrid-wrap")
            .borderless(true)
            .striped(false)
            .rows(cert_rows.clone())
            .data(Rc::new(serde_json::to_value(info).unwrap_or_default())),
    );
    if new_fp.is_empty() {
        col.add_child(Container::new().with_child(tr!(
            "Could not read the presented certificate's fingerprint."
        )));
    } else {
        col.add_child(Container::new().with_child(tr!(
            "Only the fingerprint is stored, so PDM cannot confirm this is a legitimate renewal."
        )));
    }
    col.into()
}

fn columns(
    link: yew::html::Scope<PdmRemoteCertCheck>,
    store: TreeStore<CertTreeEntry>,
    cert_rows: Rc<Vec<KVGridRow>>,
) -> Rc<Vec<DataTableHeader<CertTreeEntry>>> {
    // Node row: hostname; Detail row: cert detail spanning all 3 columns via colspan=3
    // (pwt's data_table/row.rs advances column_num by the colspan so Status/Action are skipped).
    let cert_rows_for_node = cert_rows.clone();
    let node_col = DataTableColumn::new(tr!("Node"))
        .flex(1)
        .tree_column(store)
        .render_cell(move |args: &mut DataTableCellRenderArgs<CertTreeEntry>| {
            // Clone out of `record` so the immutable borrow ends before mutating `args.config`.
            let entry = args.record().clone();
            match entry {
                CertTreeEntry::Node { hostname, .. } => html! { <b>{ hostname }</b> },
                CertTreeEntry::Detail {
                    pinned,
                    info,
                    system_trusted_pinned,
                    ..
                } => {
                    args.config.set_attribute("colspan", "3");
                    render_detail(&cert_rows_for_node, &pinned, &info, system_trusted_pinned)
                }
                CertTreeEntry::Root => html! {},
            }
        });

    let status_col =
        DataTableColumn::new(tr!("Status"))
            .width("160px")
            .render(|entry: &CertTreeEntry| {
                let CertTreeEntry::Node { status, staged, .. } = entry else {
                    return html! {};
                };
                // A staged change overrides the probe status: the row is "pending Apply".
                let (icon, token, tip) = match staged {
                    Some(Staged::Accept(_)) => (
                        Fa::from(Status::Success),
                        tr!("Accepted"),
                        tr!("New fingerprint will be stored on Apply."),
                    ),
                    Some(Staged::Clear) => (
                        Fa::from(Status::Success),
                        tr!("Will clear"),
                        tr!("Stored fingerprint will be cleared on Apply."),
                    ),
                    None => {
                        let (i, t) = status.icon_and_token();
                        (i, t, status.description())
                    }
                };
                Tooltip::new(
                    Row::new()
                        .gap(2)
                        .class(AlignItems::Center)
                        .with_child(icon)
                        .with_child(token),
                )
                .tip(tip)
                .into()
            });

    let action_col =
        DataTableColumn::new(tr!("Action"))
            .width("80px")
            .render(move |entry: &CertTreeEntry| {
                let CertTreeEntry::Node {
                    hostname,
                    status,
                    staged,
                } = entry
                else {
                    return html! {};
                };
                // Staged -> Revert; Changed -> Accept; SystemTrusted+pin -> Clear; else nothing.
                // Callbacks build a fresh Msg per click so Msg stays non-Clone (ProbeResult carries
                // anyhow::Error).
                let host = hostname.clone();
                let (icon, label, cb): (&str, String, Callback<yew::Event>) = if staged.is_some() {
                    (
                        "fa fa-fw fa-undo",
                        tr!("Revert pending change"),
                        link.callback(move |_| Msg::Revert(host.clone())),
                    )
                } else {
                    match status {
                        NodeStatus::Changed(info) => match info.fingerprint.clone() {
                            Some(fp) if !fp.is_empty() => (
                                "fa fa-fw fa-check",
                                tr!("Accept new fingerprint"),
                                link.callback(move |_| Msg::StageAccept(host.clone(), fp.clone())),
                            ),
                            _ => return html! {},
                        },
                        NodeStatus::SystemTrusted => (
                            "fa fa-fw fa-trash-o",
                            tr!("Clear stored fingerprint"),
                            link.callback(move |_| Msg::StageClear(host.clone())),
                        ),
                        _ => return html! {},
                    }
                };
                let tip = label.clone();
                Row::new()
                    .class(JustifyContent::FlexEnd)
                    .with_child(
                        Tooltip::new(
                            // ActionIcon defaults to tabindex -1; Tooltip is no accessible name.
                            ActionIcon::new(icon)
                                .tabindex(0)
                                .aria_label(label)
                                .on_activate(cb),
                        )
                        .tip(tip),
                    )
                    .into()
            });

    Rc::new(vec![node_col.into(), status_col.into(), action_col.into()])
}

/// Case- and separator-insensitive fingerprint comparison.
fn normalize_fp(fp: &str) -> String {
    fp.chars()
        .filter(|c| *c != ':')
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// KVGrid rows for a [`CertificateInfo`] (mirrors the add-wizard's certificate view).
fn cert_rows() -> Vec<KVGridRow> {
    let render_date = |_name: &str, value: &Value, _record: &Value| -> Html {
        match value.as_i64() {
            Some(value) => html! { proxmox_yew_comp::utils::render_epoch(value) },
            None => html! { value.to_string() },
        }
    };
    vec![
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
                    Ok(value) => html! { <pre>{ &value.join("\n") }</pre> },
                    _ => html! { value.to_string() },
                }
            },
        ),
    ]
}
