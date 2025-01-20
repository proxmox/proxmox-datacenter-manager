use std::rc::Rc;

use anyhow::Error;
use serde_json::json;
use yew::{
    virtual_dom::{VComp, VNode},
    Component,
};

use proxmox_yew_comp::{http_get, GuestState, Status};
use pwt::{
    css::{AlignItems, FlexFit, FlexWrap, JustifyContent},
    prelude::*,
    widget::{error_message, Button, Column, Container, Fa, Panel, Row},
    AsyncPool,
};

use pdm_api_types::resource::{GuestStatusCount, NodeStatusCount, ResourcesStatus};
use pdm_client::types::TopEntity;

use crate::{remotes::AddWizard, RemoteList};

mod top_entities;
pub use top_entities::TopEntities;

mod subscription_info;
pub use subscription_info::SubscriptionInfo;

#[derive(Properties, PartialEq)]
pub struct Dashboard {
    #[prop_or(60)]
    /// The time (in seconds) to not refresh cached data. (Default: 60)
    max_age_seconds: u64,
}

impl Dashboard {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

impl Default for Dashboard {
    fn default() -> Self {
        Self::new()
    }
}

pub enum Msg {
    LoadingFinished(Result<ResourcesStatus, Error>),
    TopEntitiesLoadResult(Result<pdm_client::types::TopEntities, proxmox_client::Error>),
    RemoteListChanged(RemoteList),
    CreateWizard(bool),
}

pub struct PdmDashboard {
    status: ResourcesStatus,
    last_error: Option<Error>,
    top_entities: Option<pdm_client::types::TopEntities>,
    last_top_entities_error: Option<proxmox_client::Error>,
    loading: bool,
    remote_list: RemoteList,
    show_wizard: bool,
    _context_listener: ContextHandle<RemoteList>,
    _async_pool: AsyncPool,
}

impl PdmDashboard {
    fn create_title_with_icon(&self, icon: &str, title: String) -> Html {
        Row::new()
            .class(AlignItems::Center)
            .gap(2)
            .with_child(Fa::new(icon))
            .with_child(title)
            .into()
    }

    fn create_node_panel(&self, icon: &str, title: String, status: &NodeStatusCount) -> Panel {
        let (status_icon, text) = match status {
            NodeStatusCount {
                online, offline, ..
            } if *offline > 0 => (
                Status::Error.to_fa_icon(),
                tr!("{0} of {1} nodes are offline", offline, online),
            ),
            NodeStatusCount { unknown, .. } if *unknown > 0 => (
                Status::Warning.to_fa_icon(),
                tr!("{0} nodes have an unknown status", unknown),
            ),
            // FIXME, get more detailed status about the failed remotes (name, type, error)?
            NodeStatusCount { online, .. } if self.status.failed_remotes > 0 => (
                Status::Unknown.to_fa_icon(),
                tr!("{0} of an unknown number of nodes online", online),
            ),
            NodeStatusCount { online, .. } => (
                Status::Success.to_fa_icon(),
                tr!("{0} nodes online", online),
            ),
        };
        Panel::new()
            .flex(1.0)
            .width(300)
            .title(self.create_title_with_icon(icon, title))
            .border(true)
            .with_child(
                Column::new()
                    .padding(4)
                    .class(FlexFit)
                    .class(AlignItems::Center)
                    .class(JustifyContent::Center)
                    .gap(2)
                    .with_child(if self.loading {
                        html! {<i class={"pwt-loading-icon"} />}
                    } else {
                        status_icon.large_4x().into()
                    })
                    .with_optional_child((!self.loading).then_some(text)),
            )
    }

    fn create_guest_panel(&self, icon: &str, title: String, status: &GuestStatusCount) -> Panel {
        Panel::new()
            .flex(1.0)
            .width(300)
            .title(self.create_title_with_icon(icon, title))
            .border(true)
            .with_child(if self.loading {
                loading_column()
            } else {
                Column::new()
                    .padding(4)
                    .gap(2)
                    .class(FlexFit)
                    .class(JustifyContent::Center)
                    .with_child(
                        Row::new()
                            .gap(2)
                            .with_child(GuestState::Running.to_fa_icon().fixed_width())
                            .with_child(tr!("running"))
                            .with_flex_spacer()
                            .with_child(Container::from_tag("span").with_child(status.running)),
                    )
                    .with_child(
                        Row::new()
                            .gap(2)
                            .with_child(GuestState::Stopped.to_fa_icon().fixed_width())
                            .with_child(tr!("stopped"))
                            .with_flex_spacer()
                            .with_child(Container::from_tag("span").with_child(status.stopped)),
                    )
                    // FIXME: show templates?
                    .with_optional_child(
                        (self.status.qemu.unknown > 0).then_some(
                            Row::new()
                                .gap(2)
                                .with_child(GuestState::Unknown.to_fa_icon().fixed_width())
                                .with_child(tr!("unknown"))
                                .with_flex_spacer()
                                .with_child(Container::from_tag("span").with_child(status.unknown)),
                        ),
                    )
            })
    }

    fn create_top_entities_panel(
        &self,
        icon: &str,
        title: String,
        metrics_title: String,
        entities: Option<&Vec<TopEntity>>,
    ) -> Panel {
        Panel::new()
            .flex(1.0)
            .width(500)
            .min_width(400)
            .border(true)
            .title(self.create_title_with_icon(icon, title))
            .with_optional_child(
                entities.map(|entities| TopEntities::new(entities.clone(), metrics_title)),
            )
            .with_optional_child(self.top_entities.is_none().then_some(loading_column()))
            .with_optional_child(
                self.last_top_entities_error
                    .as_ref()
                    .map(|err| error_message(&err.to_string())),
            )
    }
}

impl Component for PdmDashboard {
    type Message = Msg;
    type Properties = Dashboard;

    fn create(ctx: &yew::Context<Self>) -> Self {
        let link = ctx.link().clone();
        let max_age = ctx.props().max_age_seconds;

        let async_pool = AsyncPool::new();

        async_pool.spawn(async move {
            let result = http_get("/resources/status", Some(json!({"max-age": max_age}))).await;
            link.send_message(Msg::LoadingFinished(result));
        });
        async_pool.spawn({
            let link = ctx.link().clone();
            async move {
                let result = crate::pdm_client().get_top_entities().await;
                link.send_message(Msg::TopEntitiesLoadResult(result));
            }
        });
        let (remote_list, _context_listener) = ctx
            .link()
            .context(ctx.link().callback(Msg::RemoteListChanged))
            .expect("No Remote list context provided");

        Self {
            status: ResourcesStatus::default(),
            last_error: None,
            top_entities: None,
            last_top_entities_error: None,
            loading: true,
            remote_list,
            show_wizard: false,
            _context_listener,
            _async_pool: async_pool,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadingFinished(resources_status) => {
                match resources_status {
                    Ok(status) => {
                        self.last_error = None;
                        self.status = status;
                    }
                    Err(err) => self.last_error = Some(err),
                }
                self.loading = false;
                true
            }
            Msg::TopEntitiesLoadResult(res) => {
                match res {
                    Ok(data) => {
                        self.last_top_entities_error = None;
                        self.top_entities = Some(data);
                    }
                    Err(err) => self.last_top_entities_error = Some(err),
                }
                true
            }
            Msg::RemoteListChanged(remote_list) => {
                let changed = self.remote_list != remote_list;
                self.remote_list = remote_list;
                changed
            }
            Msg::CreateWizard(show) => {
                self.show_wizard = show;
                true
            }
        }
    }

    fn view(&self, _ctx: &yew::Context<Self>) -> yew::Html {
        let (remote_icon, remote_text) = match (self.status.failed_remotes, self.status.remotes) {
            (0, 0) => (Status::Warning.to_fa_icon(), tr!("No remotes configured.")),
            (0, _) => (
                Status::Success.to_fa_icon(),
                tr!("Could reach all remotes."),
            ),
            (failed, _) => (
                Status::Error.to_fa_icon(),
                tr!("{0} remotes failed to reach.", failed),
            ),
        };

        let content = Column::new()
            .class(FlexFit)
            .with_child(
                Container::new()
                    .class("pwt-content-spacer")
                    .class("pwt-flex-direction-row")
                    .class(FlexWrap::Wrap)
                    .with_child(
                        Panel::new()
                            .title(self.create_title_with_icon("server", tr!("Remotes")))
                            .flex(1.0)
                            //.border(true)
                            .width(300)
                            .min_height(175)
                            .with_tool(
                                Button::new(tr!("Add"))
                                    .icon_class("fa fa-plus-circle")
                                    .onclick(_ctx.link().callback(|_| Msg::CreateWizard(true))),
                            )
                            .with_child(
                                Column::new()
                                    .padding(4)
                                    .class(FlexFit)
                                    .class(AlignItems::Center)
                                    .class(JustifyContent::Center)
                                    .gap(2)
                                    .with_child(if self.loading {
                                        html! {<i class={"pwt-loading-icon"} />}
                                    } else {
                                        remote_icon.large_4x().into()
                                    })
                                    .with_optional_child((!self.loading).then_some(remote_text)),
                            ),
                    )
                    .with_child(self.create_node_panel(
                        "building",
                        tr!("Virtual Environment Nodes"),
                        &self.status.pve_nodes,
                    ))
                    .with_child(self.create_guest_panel(
                        "desktop",
                        tr!("Virtual Machines"),
                        &self.status.qemu,
                    ))
                    .with_child(self.create_guest_panel(
                        "cubes",
                        tr!("Linux Container"),
                        &self.status.lxc,
                    ))
                    // FIXME: add PBS support
                    //.with_child(self.create_node_panel(
                    //    "building-o",
                    //    tr!("Backup Server Nodes"),
                    //    &self.status.pbs_nodes,
                    //))
                    //.with_child(
                    //    Panel::new()
                    //        .flex(1.0)
                    //        .width(300)
                    //        .title(self.create_title_with_icon(
                    //            "floppy-o",
                    //            tr!("Backup Server Datastores"),
                    //        ))
                    //        .border(true)
                    //        .with_child(if self.loading {
                    //            Column::new()
                    //                .padding(4)
                    //                .class(FlexFit)
                    //                .class(JustifyContent::Center)
                    //                .class(AlignItems::Center)
                    //                .with_child(html! {<i class={"pwt-loading-icon"} />})
                    //        } else {
                    //            Column::new()
                    //                .padding(4)
                    //                .class(FlexFit)
                    //                .class(JustifyContent::Center)
                    //                .gap(2)
                    //                // FIXME: show more detailed status (usage?)
                    //                .with_child(
                    //                    Row::new()
                    //                        .gap(2)
                    //                        .with_child(
                    //                            StorageState::Available.to_fa_icon().fixed_width(),
                    //                        )
                    //                        .with_child(tr!("available"))
                    //                        .with_flex_spacer()
                    //                        .with_child(
                    //                            Container::from_tag("span").with_child(
                    //                                self.status.pbs_datastores.available,
                    //                            ),
                    //                        ),
                    //                )
                    //                .with_optional_child(
                    //                    (self.status.pbs_datastores.unknown > 0).then_some(
                    //                        Row::new()
                    //                            .gap(2)
                    //                            .with_child(
                    //                                StorageState::Unknown
                    //                                    .to_fa_icon()
                    //                                    .fixed_width(),
                    //                            )
                    //                            .with_child(tr!("unknown"))
                    //                            .with_flex_spacer()
                    //                            .with_child(
                    //                                Container::from_tag("span").with_child(
                    //                                    self.status.pbs_datastores.unknown,
                    //                                ),
                    //                            ),
                    //                    ),
                    //                )
                    //        }),
                    //)
                    .with_child(SubscriptionInfo::new()),
            )
            .with_child(
                Container::new()
                    .class("pwt-content-spacer")
                    .class("pwt-flex-direction-row")
                    .class("pwt-align-content-start")
                    .class(pwt::css::Flex::Fill)
                    .style("padding-top", "0")
                    .class(FlexWrap::Wrap)
                    //.min_height(175)
                    .with_child(self.create_top_entities_panel(
                        "desktop",
                        tr!("Guests With the Highest CPU Usage"),
                        tr!("CPU usage"),
                        self.top_entities.as_ref().map(|e| &e.guest_cpu),
                    ))
                    .with_child(self.create_top_entities_panel(
                        "building",
                        tr!("Nodes With the Highest CPU Usage"),
                        tr!("CPU usage"),
                        self.top_entities.as_ref().map(|e| &e.node_cpu),
                    ))
                    .with_child(self.create_top_entities_panel(
                        "building",
                        tr!("Nodes With the Highest Memory Usage"),
                        tr!("Memory usage"),
                        self.top_entities.as_ref().map(|e| &e.node_memory),
                    )),
            );

        Panel::new()
            .class(FlexFit)
            .with_child(content)
            // FIXME: make pbs also addable?
            .with_optional_child(
                self.show_wizard.then_some(
                    AddWizard::new(pdm_api_types::remotes::RemoteType::Pve)
                        .on_close(_ctx.link().callback(|_| Msg::CreateWizard(false)))
                        .on_submit(move |ctx| {
                            crate::remotes::create_remote(
                                ctx,
                                pdm_api_types::remotes::RemoteType::Pve,
                            )
                        }),
                ),
            )
            .into()
    }
}

impl From<Dashboard> for VNode {
    fn from(val: Dashboard) -> Self {
        let comp = VComp::new::<PdmDashboard>(Rc::new(val), None);
        VNode::from(comp)
    }
}

fn loading_column() -> Column {
    Column::new()
        .padding(4)
        .class(FlexFit)
        .class(JustifyContent::Center)
        .class(AlignItems::Center)
        .with_child(html! {<i class={"pwt-loading-icon"} />})
}
