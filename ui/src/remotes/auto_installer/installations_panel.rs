//! Implements the UI components for displaying an overview view of all finished/in-progress
//! installations.

use anyhow::{anyhow, Result};
use core::clone::Clone;
use std::{future::Future, pin::Pin, rc::Rc};

use pdm_api_types::auto_installer::{Installation, InstallationStatus};
use proxmox_installer_types::{post_hook::PostHookInfo, SystemInfo};
use proxmox_yew_comp::{
    percent_encoding::percent_encode_component, ConfirmButton, DataViewWindow, LoadableComponent,
    LoadableComponentContext, LoadableComponentMaster, LoadableComponentScopeExt,
    LoadableComponentState,
};
use pwt::{
    css::{Flex, FlexFit, Overflow},
    props::{
        ContainerBuilder, CssPaddingBuilder, EventSubscriber, FieldBuilder, WidgetBuilder,
        WidgetStyleBuilder,
    },
    state::{Selection, Store},
    tr,
    widget::{
        data_table::{DataTable, DataTableColumn, DataTableHeader},
        form::TextArea,
        Button, Toolbar,
    },
};
use yew::{
    virtual_dom::{Key, VComp, VNode},
    Properties,
};

use crate::pdm_client;

#[derive(Default, PartialEq, Properties)]
pub struct InstallationsPanel {}

impl From<InstallationsPanel> for VNode {
    fn from(value: InstallationsPanel) -> Self {
        let comp = VComp::new::<LoadableComponentMaster<InstallationsPanelComponent>>(
            Rc::new(value),
            None,
        );
        VNode::from(comp)
    }
}

enum Message {
    Refresh,
    SelectionChange,
    RemoveEntry,
}

#[derive(PartialEq)]
enum ViewState {
    ShowRawSystemInfo,
    ShowRawPostHookData,
}

struct InstallationsPanelComponent {
    state: LoadableComponentState<ViewState>,
    selection: Selection,
    store: Store<Installation>,
    columns: Rc<Vec<DataTableHeader<Installation>>>,
}

pwt::impl_deref_mut_property!(
    InstallationsPanelComponent,
    state,
    LoadableComponentState<ViewState>
);

impl LoadableComponent for InstallationsPanelComponent {
    type Properties = InstallationsPanel;
    type Message = Message;
    type ViewState = ViewState;

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        let selection =
            Selection::new().on_select(ctx.link().callback(|_| Message::SelectionChange));

        let store =
            Store::with_extract_key(|record: &Installation| Key::from(record.uuid.to_string()));
        store.set_sorter(|a: &Installation, b: &Installation| a.received_at.cmp(&b.received_at));

        Self {
            state: LoadableComponentState::new(),
            selection,
            store,
            columns: Rc::new(columns()),
        }
    }

    fn load(
        &self,
        _ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<()>>>> {
        let store = self.store.clone();
        Box::pin(async move {
            let data = pdm_client().get_autoinst_installations().await?;
            store.write().set_data(data);
            Ok(())
        })
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Self::Message::Refresh => {
                ctx.link().send_reload();
                false
            }
            Self::Message::SelectionChange => true,
            Self::Message::RemoveEntry => {
                if let Some(key) = self.selection.selected_key() {
                    let link = ctx.link().clone();
                    self.spawn(async move {
                        if let Err(err) = delete_entry(key).await {
                            link.show_error(tr!("Unable to delete entry"), err, true);
                        }
                        link.send_reload();
                    })
                }
                false
            }
        }
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<yew::Html> {
        let link = ctx.link();

        let selection_has_post_hook_data = self
            .selection
            .selected_key()
            .and_then(|key| {
                self.store
                    .read()
                    .lookup_record(&key)
                    .map(|data| data.post_hook_data.is_some())
            })
            .unwrap_or(false);

        let toolbar = Toolbar::new()
            .class("pwt-w-100")
            .class(Overflow::Hidden)
            .class("pwt-border-bottom")
            .with_child(
                Button::new(tr!("System Information"))
                    .disabled(self.selection.is_empty())
                    .onclick(link.change_view_callback(|_| Some(ViewState::ShowRawSystemInfo))),
            )
            .with_child(
                Button::new(tr!("Post-Installation Webhook Data"))
                    .disabled(self.selection.is_empty() || !selection_has_post_hook_data)
                    .onclick(link.change_view_callback(|_| Some(ViewState::ShowRawPostHookData))),
            )
            .with_spacer()
            .with_child(
                ConfirmButton::new(tr!("Remove"))
                    .confirm_message(tr!("Are you sure you want to remove this entry?"))
                    .disabled(self.selection.is_empty())
                    .on_activate(link.callback(|_| Message::RemoveEntry)),
            )
            .with_flex_spacer()
            .with_child(
                Button::refresh(self.loading()).onclick(ctx.link().callback(|_| Message::Refresh)),
            );

        Some(toolbar.into())
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> yew::Html {
        let link = ctx.link().clone();

        DataTable::new(self.columns.clone(), self.store.clone())
            .class(FlexFit)
            .selection(self.selection.clone())
            .on_row_dblclick({
                move |_: &mut _| {
                    link.change_view(Some(Self::ViewState::ShowRawSystemInfo));
                }
            })
            .into()
    }

    fn dialog_view(
        &self,
        ctx: &LoadableComponentContext<Self>,
        view_state: &Self::ViewState,
    ) -> Option<yew::Html> {
        let on_done = ctx.link().clone().change_view_callback(|_| None);

        let record = self
            .store
            .read()
            .lookup_record(&self.selection.selected_key()?)?
            .clone();

        Some(match view_state {
            Self::ViewState::ShowRawSystemInfo => DataViewWindow::new(tr!("System Information"))
                .on_done(on_done)
                .loader({
                    move || {
                        let info = record.info.clone();
                        async move { Ok(info) }
                    }
                })
                .renderer(|data: &SystemInfo| -> yew::Html {
                    let value = serde_json::to_string_pretty(data)
                        .unwrap_or_else(|_| "<failed to decode>".to_owned());
                    render_raw_info_container(value)
                })
                .resizable(true)
                .into(),
            Self::ViewState::ShowRawPostHookData => {
                DataViewWindow::new(tr!("Post-Installation Webhook Data"))
                    .on_done(on_done)
                    .loader({
                        move || {
                            let data = record.post_hook_data.clone();
                            async move {
                                data.ok_or_else(|| anyhow!("no post-installation webhook data"))
                            }
                        }
                    })
                    .renderer(|data: &PostHookInfo| -> yew::Html {
                        let value = serde_json::to_string_pretty(data)
                            .unwrap_or_else(|_| "<failed to decode>".to_owned());
                        render_raw_info_container(value)
                    })
                    .resizable(true)
                    .into()
            }
        })
    }
}

async fn delete_entry(key: Key) -> Result<()> {
    let id = percent_encode_component(&key.to_string());
    Ok(pdm_client().delete_autoinst_installation(&id).await?)
}

fn render_raw_info_container(value: String) -> yew::Html {
    pwt::widget::Container::new()
        .class(Flex::Fill)
        .class(Overflow::Auto)
        .padding(4)
        .with_child(
            TextArea::new()
                .width("800px")
                .read_only(true)
                .attribute("rows", "40")
                .value(value),
        )
        .into()
}

fn columns() -> Vec<DataTableHeader<Installation>> {
    vec![
        DataTableColumn::new(tr!("Received"))
            .width("170px")
            .render(|item: &Installation| {
                proxmox_yew_comp::utils::render_epoch(item.received_at).into()
            })
            .sorter(|a: &Installation, b: &Installation| a.received_at.cmp(&b.received_at))
            .sort_order(Some(false))
            .into(),
        DataTableColumn::new(tr!("Product"))
            .width("300px")
            .render(|item: &Installation| {
                format!(
                    "{} {}-{}",
                    item.info.product.fullname, item.info.iso.release, item.info.iso.isorelease
                )
                .into()
            })
            .sorter(|a: &Installation, b: &Installation| {
                a.info.product.product.cmp(&b.info.product.product)
            })
            .into(),
        DataTableColumn::new(tr!("Status"))
            .width("200px")
            .render(|item: &Installation| {
                match item.status {
                    InstallationStatus::AnswerSent => tr!("Answer sent"),
                    InstallationStatus::NoAnswerFound => tr!("No matching answer found"),
                    InstallationStatus::InProgress => tr!("In Progress"),
                    InstallationStatus::Finished => tr!("Finished"),
                }
                .into()
            })
            .sorter(|a: &Installation, b: &Installation| a.status.cmp(&b.status))
            .into(),
        DataTableColumn::new(tr!("Matched Answer"))
            .flex(1)
            .render(|item: &Installation| match &item.answer_id {
                Some(s) => s.into(),
                None => "-".into(),
            })
            .sorter(|a: &Installation, b: &Installation| a.answer_id.cmp(&b.answer_id))
            .into(),
    ]
}
