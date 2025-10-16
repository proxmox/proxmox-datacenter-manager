use std::rc::Rc;

use anyhow::Error;
use proxmox_yew_comp::{
    common_api_types::TaskStatusClass,
    http_get,
    utils::{format_duration_human, render_epoch},
    Status, TaskViewer,
};
use pwt_macros::builder;
use yew::{
    html::IntoEventCallback,
    virtual_dom::{VComp, VNode},
    Component, Properties,
};

use pwt::{
    css::FlexFit,
    prelude::*,
    widget::{
        data_table::{DataTable, DataTableColumn, DataTableHeader},
        ActionIcon, AlertDialog, Fa, Mask, Tooltip,
    },
    AsyncPool,
};
use pwt::{state::Store, tr, widget::Dialog};

use pbs_api_types::TaskListItem;
use pdm_api_types::{RemoteUpid, TaskFilters, TaskStateType};

use crate::tasks::{format_optional_remote_upid, TaskWorkerType};

#[derive(PartialEq, Properties)]
#[builder]
pub struct FilteredTasks {
    grouping: TaskWorkerType,
    task_status: TaskStatusClass,
    since: i64,

    #[prop_or_default]
    #[builder_cb(IntoEventCallback, into_event_callback, ())]
    /// Callback for closing the Dialog
    on_close: Option<Callback<()>>,
}

impl FilteredTasks {
    /// Create new instance with filters for task type and status, beginning from 'since'
    pub fn new(since: i64, grouping: TaskWorkerType, task_status: TaskStatusClass) -> Self {
        yew::props!(Self {
            since,
            grouping,
            task_status,
        })
    }
}

impl From<FilteredTasks> for VNode {
    fn from(val: FilteredTasks) -> Self {
        let comp = VComp::new::<PdmFilteredTasks>(Rc::new(val), None);
        VNode::from(comp)
    }
}

pub enum Msg {
    LoadFinished(Result<Vec<TaskListItem>, Error>),
    ShowTask(Option<(RemoteUpid, Option<i64>)>),
}

pub struct PdmFilteredTasks {
    task_store: Store<TaskListItem>,
    task_info: Option<(RemoteUpid, Option<i64>)>,
    loading: bool,
    last_error: Option<Error>,
    _async_pool: AsyncPool,
}

impl PdmFilteredTasks {
    async fn load(
        since: i64,
        status: TaskStatusClass,
        grouping: TaskWorkerType,
    ) -> Result<Vec<TaskListItem>, Error> {
        // TODO replace with pdm client call
        let status = match status {
            TaskStatusClass::Ok => TaskStateType::OK,
            TaskStatusClass::Warning => TaskStateType::Warning,
            TaskStatusClass::Error => TaskStateType::Error,
        };
        let mut filters = TaskFilters {
            since: Some(since),
            limit: 0,
            userfilter: None,
            until: None,
            typefilter: None,
            statusfilter: Some(vec![status.clone()]),

            start: 0,
            errors: false,
            running: false,
        };

        match &grouping {
            TaskWorkerType::Remote(_) => {}
            worker_type => {
                filters.typefilter = Some(worker_type.to_filter().to_string());
            }
        }

        let mut params = serde_json::to_value(filters)?;

        if let TaskWorkerType::Remote(remote) = grouping {
            params["remote"] = serde_json::Value::String(remote);
        }

        http_get("/remote-tasks/list", Some(params)).await
    }
}

impl Component for PdmFilteredTasks {
    type Message = Msg;
    type Properties = FilteredTasks;

    fn create(ctx: &Context<Self>) -> Self {
        let props = ctx.props();
        let since = props.since;
        let grouping = props.grouping.clone();
        let status = props.task_status;
        let link = ctx.link().clone();
        let _async_pool = AsyncPool::new();
        _async_pool.send_future(link, async move {
            let res = Self::load(since, status, grouping).await;
            Msg::LoadFinished(res)
        });
        Self {
            task_store: Store::with_extract_key(|item: &TaskListItem| item.upid.clone().into()),
            task_info: None,
            loading: true,
            last_error: None,
            _async_pool,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadFinished(Ok(task_list_items)) => {
                self.last_error = None;
                self.loading = false;
                self.task_store.set_data(task_list_items);
                match _ctx.props().grouping.clone() {
                    TaskWorkerType::Remote(_) => {}
                    worker_type => {
                        self.task_store.set_filter(move |entry: &TaskListItem| {
                            worker_type == TaskWorkerType::new_from_str(&entry.worker_type)
                        });
                    }
                }
            }
            Msg::LoadFinished(Err(err)) => {
                self.loading = false;
                self.last_error = Some(err);
            }
            Msg::ShowTask(task) => {
                self.task_info = task;
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        if let Some(err) = &self.last_error {
            return AlertDialog::new(err.to_string())
                .on_close(ctx.props().on_close.clone())
                .into();
        }

        if let Some((upid, endtime)) = &self.task_info {
            // TODO PBS
            let base_url = format!("/pve/remotes/{}/tasks", upid.remote());
            TaskViewer::new(upid.to_string())
                .endtime(endtime)
                .base_url(base_url)
                .on_close({
                    let link = ctx.link().clone();
                    move |_| link.send_message(Msg::ShowTask(None))
                })
                .into()
        } else {
            let title = format!(
                "{} - {}",
                props.grouping.to_title(),
                match props.task_status {
                    TaskStatusClass::Ok => tr!("OK"),
                    TaskStatusClass::Warning => tr!("Warning" | "Warnings" % 1),
                    TaskStatusClass::Error => tr!("Error"),
                },
            );
            Dialog::new(title)
                .key(format!("filtered-tasks-{}", self.loading)) // recenters when loading
                .min_width(800)
                .min_height(600)
                .max_height("90vh") // max 90% of the screen height
                .resizable(true)
                .on_close(props.on_close.clone())
                .with_child(
                    Mask::new(
                        DataTable::new(filtered_tasks_columns(ctx), self.task_store.clone())
                            .class(FlexFit),
                    )
                    .class(FlexFit)
                    .visible(self.loading),
                )
                .into()
        }
    }
}

fn filtered_tasks_columns(
    ctx: &Context<PdmFilteredTasks>,
) -> Rc<Vec<DataTableHeader<TaskListItem>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Remote"))
            .width("minmax(150px, 1fr)")
            .get_property_owned(
                |item: &TaskListItem| match item.upid.parse::<RemoteUpid>() {
                    Ok(upid) => upid.remote().to_string(),
                    Err(_) => String::new(),
                },
            )
            .into(),
        DataTableColumn::new(tr!("Task"))
            .flex(2)
            .get_property_owned(|item: &TaskListItem| {
                format_optional_remote_upid(&item.upid, false)
            })
            .into(),
        DataTableColumn::new(tr!("Start Time"))
            .sort_order(false)
            .width("200px")
            .get_property_owned(|item: &TaskListItem| render_epoch(item.starttime))
            .into(),
        DataTableColumn::new(tr!("Duration"))
            .sorter(|a: &TaskListItem, b: &TaskListItem| {
                let duration_a = match a.endtime {
                    Some(endtime) => endtime - a.starttime,
                    None => i64::MAX,
                };
                let duration_b = match b.endtime {
                    Some(endtime) => endtime - b.starttime,
                    None => i64::MAX,
                };
                duration_a.cmp(&duration_b)
            })
            .render(|item: &TaskListItem| {
                let duration = match item.endtime {
                    Some(endtime) => endtime - item.starttime,
                    None => return String::from("-").into(),
                };
                format_duration_human(duration as f64).into()
            })
            .into(),
        DataTableColumn::new(tr!("Status"))
            .justify("center")
            .render(|item: &TaskListItem| {
                let text = item.status.as_deref().unwrap_or("");
                let icon = match text.into() {
                    TaskStatusClass::Ok => Status::Success,
                    TaskStatusClass::Warning => Status::Warning,
                    TaskStatusClass::Error => Status::Error,
                };
                Fa::from(icon).into()
            })
            .into(),
        DataTableColumn::new(tr!("Action"))
            .justify("center")
            .render({
                let link = ctx.link().clone();
                move |item: &TaskListItem| {
                    let upid = item.upid.clone();
                    let endtime = item.endtime;
                    let link = link.clone();
                    let icon = ActionIcon::new("fa fa-chevron-right").on_activate(move |_| {
                        if let Ok(upid) = upid.parse::<RemoteUpid>() {
                            link.send_message(Msg::ShowTask(Some((upid, endtime))));
                        }
                    });
                    Tooltip::new(icon).tip(tr!("Open Task")).into()
                }
            })
            .into(),
    ])
}
