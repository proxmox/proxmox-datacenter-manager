use std::rc::Rc;

use anyhow::Error;
use serde_json::json;
use yew::{
    virtual_dom::{VComp, VNode},
    Component, Properties,
};

use proxmox_yew_comp::http_get;
use pwt::{
    css::{AlignItems, FontColor},
    widget::{Column, Fa, Panel, Row},
};
use pwt::{prelude::*, widget::Mask};

use pdm_api_types::resource::ResourcesStatus;

#[derive(Properties, PartialEq)]
pub struct ResourceStatusPanel;

impl ResourceStatusPanel {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for ResourceStatusPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl From<ResourceStatusPanel> for VNode {
    fn from(val: ResourceStatusPanel) -> Self {
        let comp = VComp::new::<PdmResourceStatusPanel>(Rc::new(val), None);
        VNode::from(comp)
    }
}

pub enum Msg {
    LoadingFinished(Result<ResourcesStatus, Error>),
}

pub struct PdmResourceStatusPanel {
    loaded: bool,
    last_error: Option<Error>,
    status: ResourcesStatus,
}

impl Component for PdmResourceStatusPanel {
    type Message = Msg;

    type Properties = ResourceStatusPanel;

    fn create(ctx: &yew::Context<Self>) -> Self {
        let link = ctx.link().clone();
        wasm_bindgen_futures::spawn_local(async move {
            let result = http_get("/resources/status", Some(json!({"max-age":60}))).await;
            link.send_message(Msg::LoadingFinished(result));
        });
        Self {
            loaded: false,
            last_error: None,
            status: Default::default(),
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadingFinished(resource_count) => {
                match resource_count {
                    Ok(counts) => {
                        self.last_error = None;
                        self.status = counts;
                    }
                    Err(err) => self.last_error = Some(err),
                }
                self.loaded = true;
            }
        }
        true
    }

    fn view(&self, _ctx: &yew::Context<Self>) -> yew::Html {
        let content = match &self.last_error {
            Some(err) => Column::new().padding(4).with_child(
                Row::new()
                    .gap(2)
                    .class(AlignItems::Center)
                    .with_child(
                        Fa::new("exclamation-triangle")
                            .large_2x()
                            .class(FontColor::Error),
                    )
                    .with_child(err.to_string()),
            ),
            None => Column::new()
                .padding(4)
                .gap(4)
                .with_child(
                    Row::new()
                        .gap(2)
                        .with_child(Fa::new("building-o"))
                        .with_child(tr!("PVE Nodes"))
                        .with_flex_spacer()
                        .with_child(
                            Column::new()
                                .width(150)
                                .gap(2)
                                .with_child(
                                    Row::new()
                                        .gap(2)
                                        .with_child(
                                            Fa::new("check-circle").class(FontColor::Success),
                                        )
                                        .with_child(html! {<span>{tr!("online")}</span>})
                                        .with_flex_spacer()
                                        .with_child(
                                            html! {<span>{self.status.pve_nodes.online}</span>},
                                        ),
                                )
                                .with_child(
                                    Row::new()
                                        .gap(2)
                                        .with_child(Fa::new("times-circle").class(FontColor::Error))
                                        .with_child(html! {<span>{tr!("offline")}</span>})
                                        .with_flex_spacer()
                                        .with_child(
                                            html! {<span>{self.status.pve_nodes.offline}</span>},
                                        ),
                                )
                                .with_child(
                                    Row::new()
                                        .gap(2)
                                        .with_child(
                                            Fa::new("question-circle").class(FontColor::Warning),
                                        )
                                        .with_child(html! {<span>{tr!("unknown")}</span>})
                                        .with_flex_spacer()
                                        .with_child(
                                            html! {<span>{self.status.pve_nodes.unknown}</span>},
                                        ),
                                ),
                        ),
                )
                .with_child(
                    Row::new()
                        .gap(2)
                        .with_child(Fa::new("desktop"))
                        .with_child(tr!("QEMU Guests"))
                        .with_flex_spacer()
                        .with_child(
                            Column::new()
                                .width(150)
                                .gap(2)
                                .with_child(
                                    Row::new()
                                        .gap(2)
                                        .with_child(Fa::new("play").class(FontColor::Success))
                                        .with_child(html! {<span>{tr!("running")}</span>})
                                        .with_flex_spacer()
                                        .with_child(
                                            html! {<span>{self.status.qemu.running}</span>},
                                        ),
                                )
                                .with_child(
                                    Row::new()
                                        .gap(2)
                                        .with_child(Fa::new("stop"))
                                        .with_child(html! {<span>{tr!("stopped")}</span>})
                                        .with_flex_spacer()
                                        .with_child(
                                            html! {<span>{self.status.qemu.stopped}</span>},
                                        ),
                                ), // FIXME: templates not yet in pve-api-types
                                   //.with_child(
                                   //    Row::new()
                                   //        .gap(2)
                                   //        .with_child(Fa::new("circle"))
                                   //        .with_child(html! {<span>{tr!("Templates")}</span>})
                                   //        .with_flex_spacer()
                                   //        .with_child(
                                   //            html! {<span>{self.status.qemu.template}</span>},
                                   //        ),
                                   //),
                        ),
                )
                .with_child(
                    Row::new()
                        .gap(2)
                        .with_child(Fa::new("cube"))
                        .with_child(tr!("LXC Guests"))
                        .with_flex_spacer()
                        .with_child(
                            Column::new()
                                .width(150)
                                .gap(2)
                                .with_child(
                                    Row::new()
                                        .gap(2)
                                        .with_child(Fa::new("play").class(FontColor::Success))
                                        .with_child(html! {<span>{tr!("running")}</span>})
                                        .with_flex_spacer()
                                        .with_child(html! {<span>{self.status.lxc.running}</span>}),
                                )
                                .with_child(
                                    Row::new()
                                        .gap(2)
                                        .with_child(Fa::new("stop"))
                                        .with_child(html! {<span>{tr!("stopped")}</span>})
                                        .with_flex_spacer()
                                        .with_child(html! {<span>{self.status.lxc.stopped}</span>}),
                                ), // FIXME: templates not yet in pve-api-types
                                   //.with_child(
                                   //    Row::new()
                                   //        .gap(2)
                                   //        .with_child(Fa::new("circle"))
                                   //        .with_child(html! {<span>{tr!("Templates")}</span>})
                                   //        .with_flex_spacer()
                                   //        .with_child(
                                   //            html! {<span>{self.status.qemu.template}</span>},
                                   //        ),
                                   //),
                        ),
                ),
        };
        Mask::new(
            Panel::new()
                .width(350)
                .border(true)
                .title(tr!("Resource Count"))
                .with_child(content),
        )
        .visible(!self.loaded)
        .into()
    }
}
