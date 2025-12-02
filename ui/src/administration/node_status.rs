use std::rc::Rc;

use anyhow::Error;
use yew::virtual_dom::{VComp, VNode};

use proxmox_node_status::NodePowerCommand;
use proxmox_yew_comp::utils::copy_text_to_clipboard;
use proxmox_yew_comp::{http_post, ConfirmButton, NodeStatusPanel};
use pwt::prelude::*;
use pwt::widget::{Button, Column, Container, Row};
use pwt::AsyncAbortGuard;

#[derive(Properties, Clone, PartialEq)]
pub(crate) struct NodeStatus {}

impl NodeStatus {
    pub(crate) fn new() -> Self {
        yew::props!(Self {})
    }
}

impl From<NodeStatus> for VNode {
    fn from(value: NodeStatus) -> Self {
        VComp::new::<PdmNodeStatus>(Rc::new(value), None).into()
    }
}

enum Msg {
    Reload,
    Error(Error),
    RebootOrShutdown(NodePowerCommand),
    ShowSystemReport(bool),
    ShowPackageVersions(bool),
}

struct PdmNodeStatus {
    error: Option<Error>,
    abort_guard: Option<AsyncAbortGuard>,
    show_system_report: bool,
    show_package_versions: bool,
}

impl PdmNodeStatus {
    fn change_power_state(&mut self, ctx: &yew::Context<Self>, command: NodePowerCommand) {
        let link = ctx.link().clone();
        self.abort_guard.replace(AsyncAbortGuard::spawn(async move {
            let data = Some(serde_json::json!({"command": command}));

            let res = match http_post("/nodes/localhost/status", data).await {
                Ok(()) => Msg::Reload,
                Err(e) => Msg::Error(e),
            };

            link.send_message(res);
        }));
    }

    fn create_system_report_dialog(&self, ctx: &yew::Context<Self>) -> Html {
        // copied over from subscription_panel in proxmox-yew-comp; TODO: create own component and
        // provide download functionallity.
        proxmox_yew_comp::DataViewWindow::new(tr!("System Report"))
            .width(800)
            .height(640)
            .loader("/nodes/localhost/report")
            .renderer(|report: &String| {
                let report = report.to_owned();
                Column::new()
                    .class(pwt::css::FlexFit)
                    .with_child(
                        Container::from_tag("pre")
                            .class("pwt-flex-fit pwt-font-monospace")
                            .padding(2)
                            .style("line-height", "normal")
                            .with_child(&report),
                    )
                    .with_child(
                        Row::new().padding(2).with_flex_spacer().with_child(
                            Button::new(tr!("Copy to clipboard"))
                                .icon_class("fa fa-clipboard")
                                .class(pwt::css::ColorScheme::Primary)
                                .on_activate(move |_| copy_text_to_clipboard(&report)),
                        ),
                    )
                    .into()
            })
            .on_done(ctx.link().callback(|_| Msg::ShowSystemReport(false)))
            .into()
    }

    fn create_package_version_dialog(&self, ctx: &yew::Context<Self>) -> Html {
        // TODO: factor out to dedicated helper/component in proxmox-yew-comp
        proxmox_yew_comp::DataViewWindow::new(tr!("Package Versions"))
            .width(600)
            .height(600)
            .loader("/nodes/localhost/apt/versions")
            .renderer(|versions: &serde_json::Value| {
                use std::fmt::Write;

                let mut text = String::new();
                if let Some(pkgs) = versions.as_array() {
                    // loosly adapted from our JS code in proxmox-widget-toolkit
                    for pkg in pkgs {
                        let old_version =
                            pkg.get("OldVersion").and_then(|v| v.as_str()).unwrap_or("");
                        let current_state = pkg
                            .get("CurrentState")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let name = pkg.get("Package").and_then(|v| v.as_str()).unwrap_or("");
                        let extra_info =
                            pkg.get("ExtraInfo").and_then(|v| v.as_str()).unwrap_or("");

                        let version = if !old_version.is_empty() && old_version != "unknown" {
                            old_version
                        } else if current_state == "ConfigFiles" {
                            "residual config"
                        } else {
                            "not correctly installed"
                        };

                        if !extra_info.is_empty() {
                            let _ = writeln!(text, "{name}: {version} ({extra_info})");
                        } else {
                            let _ = writeln!(text, "{name}: {version}");
                        }
                    }
                }
                Column::new()
                    .class(pwt::css::FlexFit)
                    .with_child(
                        Container::from_tag("pre")
                            .class("pwt-flex-fit pwt-font-monospace")
                            .padding(2)
                            .style("line-height", "normal")
                            .with_child(&text),
                    )
                    .with_child(
                        Row::new().padding(2).with_flex_spacer().with_child(
                            Button::new(tr!("Copy to clipboard"))
                                .icon_class("fa fa-clipboard")
                                .class(pwt::css::ColorScheme::Primary)
                                .on_activate(move |_| copy_text_to_clipboard(&text)),
                        ),
                    )
                    .into()
            })
            .on_done(ctx.link().callback(|_| Msg::ShowPackageVersions(false)))
            .into()
    }
}

impl Component for PdmNodeStatus {
    type Message = Msg;
    type Properties = NodeStatus;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self {
            error: None,
            abort_guard: None,
            show_system_report: false,
            show_package_versions: false,
        }
    }

    fn update(&mut self, ctx: &yew::Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::RebootOrShutdown(command) => {
                self.change_power_state(ctx, command);
                false
            }
            Msg::Error(e) => {
                self.error = Some(e);
                true
            }
            Msg::Reload => true,
            Msg::ShowSystemReport(show_system_report) => {
                self.show_system_report = show_system_report;
                true
            }
            Msg::ShowPackageVersions(show_package_versions) => {
                self.show_package_versions = show_package_versions;
                true
            }
        }
    }

    fn view(&self, ctx: &yew::Context<Self>) -> Html {
        Column::new()
            .class(pwt::css::FlexFit)
            .with_child(
                Container::new()
                    .class("pwt-content-spacer-padding")
                    .class("pwt-content-spacer-colors")
                    .class("pwt-default-colors")
                    .padding_bottom(0)
                    .with_child(
                        Row::new()
                            .gap(1)
                            .with_child(
                                Button::new(tr!("Package Versions"))
                                    .class(pwt::css::ColorScheme::Neutral)
                                    .icon_class("fa fa-gift")
                                    .onclick(
                                        ctx.link().callback(|_| Msg::ShowPackageVersions(true)),
                                    ),
                            )
                            .with_child(
                                Button::new(tr!("System Report"))
                                    .class(pwt::css::ColorScheme::Neutral)
                                    .icon_class("fa fa-stethoscope")
                                    .onclick(ctx.link().callback(|_| Msg::ShowSystemReport(true))),
                            )
                            .with_flex_spacer()
                            .with_child(
                                ConfirmButton::new(tr!("Reboot Datacenter Manager"))
                                    .confirm_message(tr!(
                                        "Are you sure you want to reboot the node?"
                                    ))
                                    .on_activate(ctx.link().callback(|_| {
                                        Msg::RebootOrShutdown(NodePowerCommand::Reboot)
                                    }))
                                    .class(pwt::css::ColorScheme::Neutral)
                                    .icon_class("fa fa-undo"),
                            )
                            .with_child(
                                ConfirmButton::new(tr!("Shutdown Datacenter Manager"))
                                    .confirm_message(tr!(
                                        "Are you sure you want to shut down the node?"
                                    ))
                                    .on_activate(ctx.link().callback(|_| {
                                        Msg::RebootOrShutdown(NodePowerCommand::Shutdown)
                                    }))
                                    .class(pwt::css::ColorScheme::Neutral)
                                    .icon_class("fa fa-power-off"),
                            ),
                    ),
            )
            .with_child(
                Row::new()
                    .class("pwt-content-spacer-padding")
                    .class("pwt-content-spacer-colors")
                    .class("pwt-default-colors")
                    .class(pwt::css::FlexFit)
                    .with_child(NodeStatusPanel::new().status_base_url("/nodes/localhost/status")),
            )
            .with_optional_child(
                self.show_system_report
                    .then_some(self.create_system_report_dialog(ctx)),
            )
            .with_optional_child(
                self.show_package_versions
                    .then_some(self.create_package_version_dialog(ctx)),
            )
            .into()
    }
}
