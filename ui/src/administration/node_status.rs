use std::rc::Rc;

use anyhow::Error;
use yew::virtual_dom::{VComp, VNode};

use proxmox_node_status::NodePowerCommand;
use proxmox_yew_comp::{http_post, ConfirmButton, NodeStatusPanel};
use pwt::prelude::*;
use pwt::widget::{Column, Container, Row};
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
}

struct PdmNodeStatus {
    error: Option<Error>,
    abort_guard: Option<AsyncAbortGuard>,
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
}

impl Component for PdmNodeStatus {
    type Message = Msg;
    type Properties = NodeStatus;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self {
            error: None,
            abort_guard: None,
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
                                ConfirmButton::new(tr!("Reboot"))
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
                                ConfirmButton::new(tr!("Shutdown"))
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
            .into()
    }
}
