//! Implements the UI for the auto-installer answer editing panel.

use anyhow::Result;
use core::clone::Clone;
use std::{future::Future, pin::Pin, rc::Rc};
use yew::{
    virtual_dom::{Key, VComp, VNode},
    Properties,
};

use pdm_api_types::auto_installer::{
    AnswerToken, AnswerTokenCreateResult, PreparedInstallationConfig,
};
use proxmox_yew_comp::{
    percent_encoding::percent_encode_component, ConfirmButton, LoadableComponent,
    LoadableComponentContext, LoadableComponentMaster, LoadableComponentScopeExt,
    LoadableComponentState,
};
use pwt::{
    props::{ContainerBuilder, EventSubscriber, WidgetBuilder},
    state::{Selection, Store},
    tr,
    widget::{
        data_table::{DataTable, DataTableColumn, DataTableHeader},
        Button, Fa, Toolbar,
    },
};

use super::{
    prepared_answer_add_wizard::AddAnswerWizardProperties,
    prepared_answer_edit_window::EditAnswerWindowProperties,
};
use crate::{pdm_client, remotes::auto_installer::prepared_answer_form::render_show_secret_dialog};

#[derive(Default, PartialEq, Properties)]
pub struct PreparedAnswersPanel {}

impl From<PreparedAnswersPanel> for VNode {
    fn from(value: PreparedAnswersPanel) -> Self {
        let comp = VComp::new::<LoadableComponentMaster<PreparedAnswersPanelComponent>>(
            Rc::new(value),
            None,
        );
        VNode::from(comp)
    }
}

#[derive(PartialEq)]
enum ViewState {
    Create,
    Copy,
    Edit,
    DisplaySecret {
        config_id: String,
        token: AnswerToken,
        secret: String,
    },
}

#[derive(PartialEq)]
enum Message {
    SelectionChange,
    RemoveEntry,
    DisplaySecret {
        config_id: String,
        token: AnswerToken,
        secret: String,
    },
    FingerprintLoaded(Option<String>),
}

struct PreparedAnswersPanelComponent {
    state: LoadableComponentState<ViewState>,
    selection: Selection,
    store: Store<PreparedInstallationConfig>,
    columns: Rc<Vec<DataTableHeader<PreparedInstallationConfig>>>,
    fingerprint: Option<String>,
}

pwt::impl_deref_mut_property!(
    PreparedAnswersPanelComponent,
    state,
    LoadableComponentState<ViewState>
);

impl LoadableComponent for PreparedAnswersPanelComponent {
    type Properties = PreparedAnswersPanel;
    type Message = Message;
    type ViewState = ViewState;

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        let store = Store::with_extract_key(|record: &PreparedInstallationConfig| {
            Key::from(record.id.to_string())
        });
        store.set_sorter(
            |a: &PreparedInstallationConfig, b: &PreparedInstallationConfig| a.id.cmp(&b.id),
        );

        let link = ctx.link().clone();
        ctx.link().spawn(async move {
            link.send_message(Message::FingerprintLoaded(
                pdm_client()
                    .certificate_info()
                    .await
                    .ok()
                    .and_then(|mut c| c.pop().and_then(|c| c.fingerprint)),
            ));
        });

        Self {
            state: LoadableComponentState::new(),
            selection: Selection::new()
                .on_select(ctx.link().callback(|_| Message::SelectionChange)),
            store,
            columns: Rc::new(columns()),
            fingerprint: None,
        }
    }

    fn load(
        &self,
        _ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<()>>>> {
        let store = self.store.clone();
        Box::pin(async move {
            let data = pdm_client().get_autoinst_prepared_answers().await?;
            store.write().set_data(data);
            Ok(())
        })
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Message) -> bool {
        let link = ctx.link().clone();

        match msg {
            Message::SelectionChange => true,
            Message::RemoveEntry => {
                if let Some(key) = self.selection.selected_key() {
                    self.spawn(async move {
                        if let Err(err) = pdm_client()
                            .delete_autoinst_prepared_answer(&percent_encode_component(
                                &key.to_string(),
                            ))
                            .await
                        {
                            link.show_error(tr!("Unable to delete entry"), err, true);
                        }
                        link.send_reload();
                    })
                }
                false
            }
            Message::DisplaySecret {
                config_id,
                token,
                secret,
            } => {
                link.change_view(Some(Self::ViewState::DisplaySecret {
                    config_id,
                    token,
                    secret,
                }));
                false
            }
            Message::FingerprintLoaded(fp) => {
                self.fingerprint = fp;
                false
            }
        }
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<yew::Html> {
        let link = ctx.link().clone();

        let toolbar = Toolbar::new()
            .class("pwt-w-100")
            .class(pwt::css::Overflow::Hidden)
            .class("pwt-border-bottom")
            .with_child(
                Button::new(tr!("Add"))
                    .onclick(link.change_view_callback(|_| Some(ViewState::Create))),
            )
            .with_spacer()
            .with_child(
                Button::new(tr!("Copy"))
                    .onclick(link.change_view_callback(|_| Some(ViewState::Copy))),
            )
            .with_child(
                Button::new(tr!("Edit"))
                    .disabled(self.selection.is_empty())
                    .onclick(link.change_view_callback(|_| Some(ViewState::Edit))),
            )
            .with_child(
                ConfirmButton::new(tr!("Remove"))
                    .confirm_message(tr!("Are you sure you want to remove this entry?"))
                    .disabled(self.selection.is_empty())
                    .on_activate(link.callback(|_| Message::RemoveEntry)),
            );

        Some(toolbar.into())
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> yew::Html {
        let link = ctx.link().clone();

        DataTable::new(self.columns.clone(), self.store.clone())
            .class(pwt::css::FlexFit)
            .selection(self.selection.clone())
            .on_row_dblclick(move |_: &mut _| link.change_view(Some(Self::ViewState::Edit)))
            .into()
    }

    fn dialog_view(
        &self,
        ctx: &LoadableComponentContext<Self>,
        view_state: &Self::ViewState,
    ) -> Option<yew::Html> {
        let link = ctx.link().clone();
        let on_submit_result = ctx.link().callback(
            move |(config, new_token): (
                PreparedInstallationConfig,
                Option<AnswerTokenCreateResult>,
            )| {
                if let Some(token) = new_token {
                    Self::Message::DisplaySecret {
                        config_id: config.id,
                        token: token.token,
                        secret: token.secret,
                    }
                } else {
                    link.change_view(None);
                    link.send_reload();
                    Self::Message::SelectionChange
                }
            },
        );

        let on_close = ctx.link().change_view_callback(|_| None);

        match view_state {
            Self::ViewState::Create => Some(
                AddAnswerWizardProperties::new(self.fingerprint.clone())
                    .on_submit_result(on_submit_result)
                    .on_close(on_close)
                    .into(),
            ),
            Self::ViewState::Copy => {
                let mut record = self
                    .store
                    .read()
                    .lookup_record(&self.selection.selected_key()?)?
                    .clone();

                record.id += " (copy)";
                Some(
                    AddAnswerWizardProperties::with(record)
                        .on_submit_result(on_submit_result)
                        .on_close(on_close)
                        .into(),
                )
            }
            Self::ViewState::Edit => {
                let record = self
                    .store
                    .read()
                    .lookup_record(&self.selection.selected_key()?)?
                    .clone();

                Some(
                    EditAnswerWindowProperties::new(record)
                        .on_submit_result(on_submit_result)
                        .on_close(on_close)
                        .into(),
                )
            }
            Self::ViewState::DisplaySecret {
                config_id,
                token,
                secret,
            } => render_show_secret_dialog(
                Some(config_id),
                token,
                secret,
                &self.fingerprint,
                on_close,
            ),
        }
    }
}

fn columns() -> Vec<DataTableHeader<PreparedInstallationConfig>> {
    vec![
        DataTableColumn::new(tr!("ID"))
            .width("320px")
            .render(|item: &PreparedInstallationConfig| item.id.as_str().into())
            .sorter(
                |a: &PreparedInstallationConfig, b: &PreparedInstallationConfig| a.id.cmp(&b.id),
            )
            .sort_order(Some(true))
            .into(),
        DataTableColumn::new(tr!("Default"))
            .width("80px")
            .render(|item: &PreparedInstallationConfig| {
                if item.is_default {
                    Fa::new("check").into()
                } else {
                    Fa::new("times").into()
                }
            })
            .into(),
        DataTableColumn::new(tr!("Target filter"))
            .flex(1)
            .render(|item: &PreparedInstallationConfig| {
                if item.target_filter.is_empty() {
                    "-".into()
                } else {
                    item.target_filter
                        .iter()
                        .fold(String::new(), |acc, (k, v)| {
                            if acc.is_empty() {
                                format!("{k}={v}")
                            } else {
                                format!("{acc}, {k}={v}")
                            }
                        })
                        .into()
                }
            })
            .into(),
        DataTableColumn::new(tr!("Authorized Tokens"))
            .flex(1)
            .render(|item: &PreparedInstallationConfig| item.authorized_tokens.join(", ").into())
            .into(),
    ]
}
