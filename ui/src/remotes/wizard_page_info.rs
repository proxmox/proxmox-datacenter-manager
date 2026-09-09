use std::rc::Rc;

use anyhow::Error;
use html::IntoEventCallback;
use serde::{Deserialize, Serialize};
use yew::virtual_dom::{Key, VComp, VNode};

use proxmox_schema::property_string::PropertyString;
use proxmox_yew_comp::{SchemaValidation, WizardPageRenderInfo};
use pwt::{
    AsyncPool,
    css::FlexFit,
    prelude::*,
    widget::{
        Column, Container, InputPanel, Row,
        form::{Field, FormContext, FormContextObserver, InputType, RadioButton},
    },
};

use pdm_api_types::remotes::{NodeUrl, REMOTE_ID_SCHEMA, Remote, RemoteType};

use pwt_macros::builder;

use super::connection_attempt::{ConnectionAttempt, attempt_mask, failure_notice};
use super::wizard_page_connect::ConnectParams;
use crate::{get_nodename, widget::RemoteRealmSelector};

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

    remote_type: RemoteType,
}

impl WizardPageInfo {
    pub fn new(info: WizardPageRenderInfo, remote_type: RemoteType) -> Self {
        yew::props!(Self { info, remote_type })
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
    attempt: Option<ConnectionAttempt>,
    scan_id: usize,
}

pub enum Msg {
    ToggleCreateToken(bool),
    FormChange,
    Connect,
    ConnectResult {
        scan_id: usize,
        result: Result<Remote, Error>,
    },
    Cancel,
    Retry,
    Tick,
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

async fn scan(
    connection_params: ConnectParams,
    form_ctx: FormContext,
    remote_type: RemoteType,
) -> Result<Remote, Error> {
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

    let client = crate::pdm_client();
    let mut result = match remote_type {
        RemoteType::Pve => {
            client
                .pve_scan_remote(&hostname, fingerprint.as_deref(), &authid, &token)
                .await?
        }
        RemoteType::Pbs => {
            client
                .pbs_scan_remote(&hostname, fingerprint.as_deref(), &authid, &token)
                .await?
        }
    };

    merge_initial_endpoint(&mut result, hostname, fingerprint);
    Ok(result)
}

fn merge_initial_endpoint(remote: &mut Remote, hostname: String, fingerprint: Option<String>) {
    let matching_index = remote
        .nodes
        .iter()
        .position(|node| node.hostname == hostname)
        .or_else(|| {
            let fingerprint = fingerprint.as_deref()?;
            let mut matches = remote.nodes.iter().enumerate().filter(|(_, node)| {
                node.fingerprint.as_deref().is_some_and(|node_fingerprint| {
                    node_fingerprint.eq_ignore_ascii_case(fingerprint)
                })
            });
            let (index, _) = matches.next()?;
            matches.next().is_none().then_some(index)
        });

    if let Some(index) = matching_index {
        remote.nodes[index].hostname = hostname;
        remote.nodes[index].fingerprint = fingerprint;
    } else {
        remote.nodes.insert(
            0,
            PropertyString::new(NodeUrl {
                hostname,
                fingerprint,
            }),
        );
    }

    remote
        .nodes
        .sort_by(|left, right| left.hostname.cmp(&right.hostname));
}

impl PdmWizardPageInfo {
    fn abort_scan(&mut self) {
        self.async_pool = AsyncPool::new();
        self.attempt = None;
        self.loading = false;
        self.scan_id = self.scan_id.wrapping_add(1);
    }

    /// Starts a scan for the current input. Invalid input has nothing to send, and a scan that
    /// succeeded for it would advance the wizard past a page the user still has to correct.
    fn start_scan(&mut self, ctx: &Context<Self>) {
        if !ctx.props().info.form_ctx.read().is_valid() {
            return;
        }

        self.abort_scan();
        self.update_server_info(ctx, None);
        self.loading = true;
        self.last_error = None;
        ctx.props().info.page_lock(true);

        let Some(connection_info) = ctx.props().connect_info.clone() else {
            unreachable!("Settings page must have connection info");
        };
        let form_ctx = ctx.props().info.form_ctx.clone();
        let remote_type = ctx.props().remote_type;
        let link = ctx.link().clone();
        self.attempt = Some(ConnectionAttempt::new(link.callback(|_| Msg::Tick)));
        let scan_id = self.scan_id;
        self.async_pool.spawn(async move {
            let result = scan(connection_info, form_ctx, remote_type).await;
            link.send_message(Msg::ConnectResult { scan_id, result });
        });
    }

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
            attempt: None,
            scan_id: 0,
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
                props.info.page_lock(
                    self.loading
                        || self.credentials.is_none()
                        || !props.info.form_ctx.read().is_valid(),
                );
            }
            Msg::Connect => {
                if self.server_info.is_some() {
                    props.info.page_lock(false);
                    props.info.go_to_next_page();
                    return true;
                }
                self.start_scan(ctx);
            }
            Msg::ConnectResult { scan_id, result } => {
                if scan_id != self.scan_id {
                    return false;
                }
                self.attempt = None;
                self.loading = false;
                props.info.page_lock(false);
                match result {
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
            Msg::Cancel => {
                self.abort_scan();
                props.info.page_lock(!props.info.form_ctx.read().is_valid());
            }
            Msg::Retry => {
                self.start_scan(ctx);
            }
            Msg::Tick => {
                return self.attempt.as_mut().is_some_and(ConnectionAttempt::tick);
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
        let props = ctx.props();
        let (hostname, fingerprint) = match props.connect_info.clone() {
            Some(info) => (info.hostname, info.fingerprint),
            None => (Default::default(), None),
        };
        let input_panel = InputPanel::new()
            .class(FlexFit)
            .padding(4)
            .with_field(
                tr!("Remote ID"),
                Field::new()
                    .name("id")
                    .required(true)
                    .schema(&REMOTE_ID_SCHEMA),
            )
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
                RemoteRealmSelector::new(hostname.clone(), fingerprint, props.remote_type)
                    .active(props.info.visible)
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
                    .default(match get_nodename() {
                        Some(nodename) => format!("pdm-admin-{nodename}"),
                        None => "pdm-admin".to_string(),
                    }),
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
                    .placeholder("user@realm!token-id")
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
            .with_child(Row::new().with_child(input_panel))
            .with_optional_child(self.last_error.as_ref().map(|err| {
                let on_retry = props
                    .info
                    .form_ctx
                    .read()
                    .is_valid()
                    .then(|| ctx.link().callback(|_| Msg::Retry));
                failure_notice(err, &hostname, on_retry)
            }));

        attempt_mask(
            content,
            self.attempt.as_ref(),
            &tr!("Scanning remote"),
            ctx.link().callback(|_| Msg::Cancel),
        )
        .class(FlexFit)
        .into()
    }
}

impl From<WizardPageInfo> for VNode {
    fn from(val: WizardPageInfo) -> Self {
        let comp = VComp::new::<PdmWizardPageInfo>(Rc::new(val), None);
        VNode::from(comp)
    }
}

#[cfg(test)]
mod tests {
    use pdm_api_types::Authid;
    use pdm_api_types::remotes::RemoteType;

    use super::*;

    fn remote_with_nodes(nodes: Vec<NodeUrl>) -> Remote {
        Remote {
            ty: RemoteType::Pve,
            id: "cluster".to_string(),
            nodes: nodes.into_iter().map(PropertyString::new).collect(),
            authid: Authid::root_auth_id().clone(),
            token: String::new(),
            web_url: None,
        }
    }

    fn assert_nodes(remote: &Remote, expected: &[(&str, Option<&str>)]) {
        let nodes = remote
            .nodes
            .iter()
            .map(|node| (node.hostname.as_str(), node.fingerprint.as_deref()))
            .collect::<Vec<_>>();

        assert_eq!(nodes, expected);
    }

    #[test]
    fn exact_hostname_wins_over_an_earlier_shared_fingerprint() {
        let mut remote = remote_with_nodes(vec![
            NodeUrl {
                hostname: "node-a".to_string(),
                fingerprint: Some("AA:BB:CC".to_string()),
            },
            NodeUrl {
                hostname: "node-b".to_string(),
                fingerprint: Some("AA:BB:CC".to_string()),
            },
        ]);

        merge_initial_endpoint(
            &mut remote,
            "node-b".to_string(),
            Some("aa:bb:cc".to_string()),
        );

        assert_nodes(
            &remote,
            &[("node-a", Some("AA:BB:CC")), ("node-b", Some("aa:bb:cc"))],
        );
    }

    #[test]
    fn unique_fingerprint_replaces_discovered_hostname_with_alias() {
        let mut remote = remote_with_nodes(vec![
            NodeUrl {
                hostname: "node-a".to_string(),
                fingerprint: Some("AA:BB:CC".to_string()),
            },
            NodeUrl {
                hostname: "node-b".to_string(),
                fingerprint: Some("DD:EE:FF".to_string()),
            },
        ]);

        merge_initial_endpoint(
            &mut remote,
            "10.0.0.1".to_string(),
            Some("aa:bb:cc".to_string()),
        );

        assert_nodes(
            &remote,
            &[("10.0.0.1", Some("aa:bb:cc")), ("node-b", Some("DD:EE:FF"))],
        );
    }

    #[test]
    fn shared_fingerprint_preserves_discovered_nodes_and_adds_alias() {
        let mut remote = remote_with_nodes(vec![
            NodeUrl {
                hostname: "node-a".to_string(),
                fingerprint: Some("AA:BB:CC".to_string()),
            },
            NodeUrl {
                hostname: "node-b".to_string(),
                fingerprint: Some("AA:BB:CC".to_string()),
            },
        ]);

        merge_initial_endpoint(
            &mut remote,
            "10.0.0.1".to_string(),
            Some("AA:BB:CC".to_string()),
        );

        assert_nodes(
            &remote,
            &[
                ("10.0.0.1", Some("AA:BB:CC")),
                ("node-a", Some("AA:BB:CC")),
                ("node-b", Some("AA:BB:CC")),
            ],
        );
    }

    #[test]
    fn absent_fingerprint_adds_unmatched_hostname() {
        let mut remote = remote_with_nodes(vec![NodeUrl {
            hostname: "node-a".to_string(),
            fingerprint: Some("AA:BB:CC".to_string()),
        }]);

        merge_initial_endpoint(&mut remote, "initial-address".to_string(), None);

        assert_nodes(
            &remote,
            &[("initial-address", None), ("node-a", Some("AA:BB:CC"))],
        );
    }
}
