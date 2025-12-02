use std::rc::Rc;

use yew::{
    html,
    html::IntoPropValue,
    virtual_dom::{VComp, VNode},
    AttrValue, Component, Properties,
};

use pbs_api_types::TaskListItem;
use pdm_api_types::RemoteUpid;

use proxmox_yew_comp::{utils::render_epoch_short, TaskViewer, Tasks};

use pwt::{
    css::{FlexFit, JustifyContent},
    props::{ContainerBuilder, FieldBuilder, WidgetBuilder},
    tr,
    widget::{
        data_table::{DataTableColumn, DataTableHeader},
        Column, Fa, Row,
    },
};
use pwt_macros::builder;

use crate::{tasks::format_optional_remote_upid, widget::RemoteSelector};

#[derive(PartialEq, Properties)]
#[builder]
pub struct RemoteTaskList {
    /// If given, shows only tasks of this remote
    #[prop_or_default]
    #[builder(IntoPropValue, into_prop_value)]
    remote: Option<AttrValue>,
}

impl RemoteTaskList {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

pub struct PbsRemoteTaskList {
    columns: Rc<Vec<DataTableHeader<TaskListItem>>>,
    upid: Option<(String, Option<i64>)>,
}

fn columns() -> Rc<Vec<DataTableHeader<TaskListItem>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Start Time"))
            .width("130px")
            .render(|item: &TaskListItem| render_epoch_short(item.starttime).into())
            .into(),
        DataTableColumn::new(tr!("End Time"))
            .width("130px")
            .render(|item: &TaskListItem| match item.endtime {
                Some(endtime) => render_epoch_short(endtime).into(),
                None => Row::new()
                    .class(JustifyContent::Center)
                    .with_child(Fa::new("").class("pwt-loading-icon"))
                    .into(),
            })
            .into(),
        DataTableColumn::new(tr!("User name"))
            .width("minmax(150px, 1fr)")
            .render(|item: &TaskListItem| {
                html! {&item.user}
            })
            .into(),
        DataTableColumn::new(tr!("Remote"))
            .width("minmax(150px, 1fr)")
            .render(
                |item: &TaskListItem| match item.upid.parse::<RemoteUpid>() {
                    Ok(remote) => html! {remote.remote()},
                    Err(_) => html! {{"-"}},
                },
            )
            .into(),
        DataTableColumn::new(tr!("Node"))
            .width("minmax(150px, 1fr)")
            .render(|item: &TaskListItem| {
                html! {&item.node}
            })
            .into(),
        DataTableColumn::new(tr!("Description"))
            .flex(4)
            .render(move |item: &TaskListItem| {
                format_optional_remote_upid(&item.upid, false).into()
            })
            .into(),
        DataTableColumn::new(tr!("Status"))
            .width("minmax(200px, 1fr)")
            .render(|item: &TaskListItem| match item.status.as_deref() {
                Some("RUNNING") | None => Row::new()
                    .class(JustifyContent::Center)
                    .with_child(Fa::new("").class("pwt-loading-icon"))
                    .into(),
                Some(text) => html! {text},
            })
            .into(),
    ])
}

impl Component for PbsRemoteTaskList {
    type Message = Option<(String, Option<i64>)>;
    type Properties = RemoteTaskList;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self {
            columns: columns(),
            upid: None,
        }
    }

    fn update(&mut self, _ctx: &yew::Context<Self>, msg: Self::Message) -> bool {
        self.upid = msg;
        true
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();
        let task = self
            .upid
            .as_ref()
            .and_then(|(upid, endtime)| upid.parse::<RemoteUpid>().ok().map(|upid| (upid, endtime)))
            .map(|(remote_upid, endtime)| {
                let base_url = format!(
                    "/{}/remotes/{}/tasks",
                    remote_upid.remote_type(),
                    remote_upid.remote()
                );
                TaskViewer::new(remote_upid.to_string())
                    .endtime(endtime)
                    .base_url(base_url)
                    .on_close({
                        let link = ctx.link().clone();
                        move |_| link.send_message(None)
                    })
            });

        let mut task_list = Tasks::new()
            .base_url("/remotes/tasks/list")
            .on_show_task({
                let link = ctx.link().clone();
                move |(upid_str, endtime)| link.send_message(Some((upid_str, endtime)))
            })
            .columns(self.columns.clone());

        task_list = match props.remote.clone() {
            Some(remote) => task_list.fixed_filter(("remote".into(), remote.to_string())),
            None => task_list.extra_filter(
                tr!("Remote"),
                RemoteSelector::new().name("remote").placeholder(tr!("All")),
            ),
        };

        Column::new()
            .class(FlexFit)
            .with_child(task_list)
            .with_optional_child(task)
            .into()
    }
}

impl From<RemoteTaskList> for VNode {
    fn from(val: RemoteTaskList) -> Self {
        let comp = VComp::new::<PbsRemoteTaskList>(Rc::new(val), None);
        VNode::from(comp)
    }
}
