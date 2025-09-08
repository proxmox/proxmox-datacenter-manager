use std::collections::BTreeMap;
use std::collections::HashMap;
use std::rc::Rc;

use yew::html::Scope;
use yew::virtual_dom::Key;

use proxmox_yew_comp::common_api_types::TaskStatusClass;
use pwt::css;
use pwt::prelude::*;
use pwt::props::ExtractPrimaryKey;
use pwt::state::Store;
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::{ActionIcon, Container, Tooltip};
use pwt_macros::{builder, widget};

use pdm_api_types::TaskStatistics;

use crate::tasks::TaskWorkerType;

use super::filtered_tasks::FilteredTasks;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
struct TaskSummaryItem {
    group: TaskWorkerType,
    error_count: u64,
    warning_count: u64,
    ok_count: u64,
    unknown_count: u64,
}

impl TaskSummaryItem {
    fn new(group: TaskWorkerType) -> Self {
        TaskSummaryItem {
            group,
            error_count: 0,
            warning_count: 0,
            ok_count: 0,
            unknown_count: 0,
        }
    }
}

impl ExtractPrimaryKey for TaskSummaryItem {
    fn extract_key(&self) -> Key {
        Key::from(self.group.clone())
    }
}

#[widget(comp=ProxmoxTaskSummary, @element)]
#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct TaskSummary {
    statistics: TaskStatistics,

    since: i64,

    top_remotes: Option<u32>,
}

impl TaskSummary {
    /// New Task Summary, if `top_remotes` is `Some`, group
    /// by that much remotes instead of predefined groups.
    pub fn new(statistics: TaskStatistics, since: i64, top_remotes: Option<u32>) -> Self {
        yew::props!(Self {
            statistics,
            since,
            top_remotes
        })
    }
}

pub enum Msg {
    ShowFilteredTasks(Option<(TaskWorkerType, TaskStatusClass)>), // task_tyope
}

#[derive(PartialEq)]
pub enum ViewState {}

#[doc(hidden)]
pub struct ProxmoxTaskSummary {
    store: Store<TaskSummaryItem>,
    task_filters: Option<(TaskWorkerType, TaskStatusClass)>,
}

fn extract_task_summary(data: &TaskStatistics) -> Vec<TaskSummaryItem> {
    let mut map: BTreeMap<TaskWorkerType, TaskSummaryItem> = BTreeMap::new();

    let mut insert_type = |task_type: TaskWorkerType| {
        map.insert(task_type.clone(), TaskSummaryItem::new(task_type));
    };

    insert_type(TaskWorkerType::Migrate);
    insert_type(TaskWorkerType::Qemu);
    insert_type(TaskWorkerType::Lxc);
    insert_type(TaskWorkerType::Ceph);
    insert_type(TaskWorkerType::Backup);
    insert_type(TaskWorkerType::Ha);

    for (worker_type, count) in data.by_type.iter() {
        let task_type = TaskWorkerType::new_from_str(&worker_type);

        let entry = match map.get_mut(&task_type) {
            Some(entry) => entry,
            None => continue,
        };

        entry.ok_count += count.ok;
        entry.warning_count += count.warning;
        entry.error_count += count.error;
        entry.unknown_count += count.unknown;
    }

    map.into_values().collect()
}

fn extract_task_summary_remote(data: &TaskStatistics, limit: u32) -> Vec<TaskSummaryItem> {
    let mut map: HashMap<String, TaskSummaryItem> = HashMap::new();

    for (remote, count) in data.by_remote.iter() {
        let entry = map
            .entry(remote.to_string())
            .or_insert_with(|| TaskSummaryItem::new(TaskWorkerType::Remote(remote.to_string())));

        entry.ok_count += count.ok;
        entry.warning_count += count.warning;
        entry.error_count += count.error;
        entry.unknown_count += count.unknown;
    }

    let mut list: Vec<TaskSummaryItem> = map.into_values().collect();
    list.sort_by(|a, b| {
        let a_count = a.error_count + a.warning_count + a.ok_count;
        let b_count = b.error_count + b.warning_count + b.ok_count;
        b_count.cmp(&a_count)
    });

    list.into_iter().take(limit as usize).collect()
}

fn render_counter(
    link: Scope<ProxmoxTaskSummary>,
    count: u64,
    task_type: TaskWorkerType,
    task_class: TaskStatusClass,
) -> Html {
    let (icon_class, icon_scheme, state_text) = match task_class {
        TaskStatusClass::Ok => ("fa-check", css::ColorScheme::Success, tr!("OK")),
        TaskStatusClass::Warning => (
            "fa-exclamation-triangle",
            css::ColorScheme::Warning,
            tr!("Warning" | "Warnings" % count),
        ),
        TaskStatusClass::Error => ("fa-times-circle", css::ColorScheme::Error, tr!("Error")),
    };
    let tip = tr!("Show: {0} - {1}", task_type.to_title(), state_text);

    let has_count = count > 0;
    let action = ActionIcon::new(classes!("fa", icon_class))
        .margin_end(1)
        .class(has_count.then_some(icon_scheme))
        .disabled(!has_count)
        .on_activate(move |_| {
            link.send_message(Msg::ShowFilteredTasks(Some((
                task_type.clone(),
                task_class,
            ))))
        });

    Tooltip::new(
        Container::from_tag("span")
            .with_child(action)
            .with_child(count),
    )
    .tip(has_count.then_some(tip))
    .into()
}

impl ProxmoxTaskSummary {
    fn task_summary_columns(
        &self,
        ctx: &Context<Self>,
    ) -> Rc<Vec<DataTableHeader<TaskSummaryItem>>> {
        Rc::new(vec![
            DataTableColumn::new("")
                .flex(1)
                .get_property_owned(|item: &TaskSummaryItem| item.group.to_title())
                .into(),
            DataTableColumn::new("")
                .width("100px")
                .render({
                    let link = ctx.link().clone();
                    move |item: &TaskSummaryItem| {
                        render_counter(
                            link.clone(),
                            item.error_count,
                            item.group.clone(),
                            TaskStatusClass::Error,
                        )
                    }
                })
                .into(),
            DataTableColumn::new("")
                .width("100px")
                .render({
                    let link = ctx.link().clone();
                    move |item: &TaskSummaryItem| {
                        render_counter(
                            link.clone(),
                            item.warning_count,
                            item.group.clone(),
                            TaskStatusClass::Warning,
                        )
                    }
                })
                .into(),
            DataTableColumn::new("")
                .width("100px")
                .render({
                    let link = ctx.link().clone();
                    move |item: &TaskSummaryItem| {
                        render_counter(
                            link.clone(),
                            item.ok_count,
                            item.group.clone(),
                            TaskStatusClass::Ok,
                        )
                    }
                })
                .into(),
        ])
    }

    fn update_task_statistics(&mut self, ctx: &Context<Self>) {
        let store = &self.store;

        if let Some(top_remotes) = ctx.props().top_remotes {
            store.set_data(extract_task_summary_remote(
                &ctx.props().statistics,
                top_remotes,
            ));
        } else {
            store.set_data(extract_task_summary(&ctx.props().statistics));
        }
    }
}

impl Component for ProxmoxTaskSummary {
    type Properties = TaskSummary;
    type Message = Msg;

    fn create(ctx: &Context<Self>) -> Self {
        let mut this = Self {
            store: Store::new(),
            task_filters: None,
        };

        this.update_task_statistics(ctx);
        this
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::ShowFilteredTasks(filters) => {
                self.task_filters = filters;
            }
        }
        true
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        if old_props.statistics != ctx.props().statistics {
            self.update_task_statistics(ctx);
        }

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let tasks = self.task_filters.as_ref().map(|(task_type, task_status)| {
            FilteredTasks::new(props.since, task_type.clone(), *task_status).on_close({
                let link = ctx.link().clone();
                move |_| link.send_message(Msg::ShowFilteredTasks(None))
            })
        });

        Container::new()
            .class(css::FlexFit)
            .with_child(
                DataTable::new(self.task_summary_columns(ctx), self.store.clone())
                    .class(pwt::css::FlexFit)
                    .striped(false)
                    .borderless(true)
                    .hover(true)
                    .show_header(false),
            )
            .with_optional_child(tasks)
            .into()
    }
}
