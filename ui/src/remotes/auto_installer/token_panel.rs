//! Implements the UI for the auto-installer authentication authentication token panel.

use anyhow::{Result, bail};
use core::clone::Clone;
use std::{future::Future, pin::Pin, rc::Rc};
use yew::{
    Html, Properties, html,
    virtual_dom::{Key, VComp, VNode},
};

use pdm_api_types::auto_installer::{
    AnswerToken, AnswerTokenCreateResult, AnswerTokenUpdateResult, AnswerTokenUpdater,
};
use proxmox_yew_comp::{
    ConfirmButton, EditWindow, LoadableComponent, LoadableComponentContext,
    LoadableComponentMaster, LoadableComponentScopeExt, LoadableComponentState,
    percent_encoding::percent_encode_component,
    utils::{epoch_to_input_value, render_epoch},
};
use pwt::{
    props::{ContainerBuilder, CssPaddingBuilder, EventSubscriber, FieldBuilder, WidgetBuilder},
    state::{Selection, Store},
    tr,
    widget::{
        Button, Fa, InputPanel, Toolbar,
        data_table::{DataTable, DataTableColumn, DataTableHeader},
        form::{Checkbox, Field, FormContext, InputType},
    },
};

use crate::{pdm_client, remotes::auto_installer::prepared_answer_form::render_show_secret_dialog};

#[derive(Default, PartialEq, Properties)]
pub struct AuthTokenPanel {
    /// Bumped by the parent to request a reload, for example after the prepared-answer flow
    /// auto-created a token.
    #[prop_or_default]
    pub reload_trigger: usize,
}

impl AuthTokenPanel {
    pub fn reload_trigger(mut self, reload_trigger: usize) -> Self {
        self.reload_trigger = reload_trigger;
        self
    }
}

impl From<AuthTokenPanel> for VNode {
    fn from(value: AuthTokenPanel) -> Self {
        let comp =
            VComp::new::<LoadableComponentMaster<AuthTokenPanelComponent>>(Rc::new(value), None);
        VNode::from(comp)
    }
}

#[derive(PartialEq)]
enum ViewState {
    Create,
    Edit,
    DisplaySecret { token: AnswerToken, secret: String },
}

#[derive(PartialEq)]
enum Message {
    SelectionChange,
    RemoveEntry,
    RegenerateSecret,
    FingerprintLoaded(Option<String>),
}

struct AuthTokenPanelComponent {
    state: LoadableComponentState<ViewState>,
    selection: Selection,
    store: Store<AnswerToken>,
    columns: Rc<Vec<DataTableHeader<AnswerToken>>>,
    fingerprint: Option<String>,
}

pwt::impl_deref_mut_property!(
    AuthTokenPanelComponent,
    state,
    LoadableComponentState<ViewState>
);

impl LoadableComponent for AuthTokenPanelComponent {
    type Properties = AuthTokenPanel;
    type Message = Message;
    type ViewState = ViewState;

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        let store =
            Store::with_extract_key(|record: &AnswerToken| Key::from(record.id.to_string()));
        store.set_sorter(|a: &AnswerToken, b: &AnswerToken| a.id.cmp(&b.id));

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
            let data = pdm_client().get_autoinst_tokens().await?;
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
                            .delete_autoinst_token(&percent_encode_component(&key.to_string()))
                            .await
                        {
                            link.show_error(tr!("Unable to delete entry"), err, true);
                        }
                        link.send_reload();
                    })
                }
                false
            }
            Message::RegenerateSecret => {
                if let Some(key) = self.selection.selected_key() {
                    self.spawn(async move {
                        match regenerate_token_secret(&key.to_string()).await {
                            Ok(AnswerTokenUpdateResult {
                                token,
                                secret: Some(secret),
                            }) => {
                                link.change_view(Some(ViewState::DisplaySecret { token, secret }))
                            }
                            Ok(_) => link.show_error(
                                tr!("Failed to regenerate secret"),
                                tr!("Received no new secret"),
                                true,
                            ),
                            Err(err) => {
                                link.show_error(tr!("Failed to regenerate secret"), err, true)
                            }
                        }
                        link.send_reload();
                    })
                }
                false
            }
            Message::FingerprintLoaded(fingerprint) => {
                self.fingerprint = fingerprint;
                false
            }
        }
    }

    fn changed(
        &mut self,
        ctx: &LoadableComponentContext<Self>,
        old_props: &Self::Properties,
    ) -> bool {
        if ctx.props().reload_trigger != old_props.reload_trigger {
            ctx.link().send_reload();
        }
        true
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
                Button::new(tr!("Edit"))
                    .disabled(self.selection.is_empty())
                    .onclick(link.change_view_callback(|_| Some(ViewState::Edit))),
            )
            .with_child(
                ConfirmButton::new(tr!("Remove"))
                    .confirm_message(tr!("Are you sure you want to remove this entry?"))
                    .disabled(self.selection.is_empty())
                    .on_activate(link.callback(|_| Message::RemoveEntry)),
            )
            .with_spacer()
            .with_child(
                ConfirmButton::new(tr!("Regenerate Secret"))
                    .confirm_message(tr!(
                        "Do you want to regenerate the secret of the selected token? \
                        All existing ISOs with this token will lose access!"
                    ))
                    .disabled(self.selection.is_empty())
                    .dangerous(true)
                    .on_activate(link.callback(|_| Message::RegenerateSecret)),
            )
            .with_flex_spacer()
            .with_child(Button::refresh(self.loading()).onclick({
                let link = ctx.link().clone();
                move |_| link.send_reload()
            }));

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
        match view_state {
            Self::ViewState::Create => self.create_add_dialog(ctx),
            Self::ViewState::Edit => self.create_edit_dialog(ctx),
            Self::ViewState::DisplaySecret { token, secret } => render_show_secret_dialog(
                None,
                None,
                token,
                secret,
                &self.fingerprint,
                ctx.link().change_view_callback(|_| None),
            ),
        }
    }
}

impl AuthTokenPanelComponent {
    fn create_add_dialog(&self, ctx: &LoadableComponentContext<Self>) -> Option<yew::Html> {
        let window = EditWindow::new(tr!("Add") + ": " + &tr!("Token"))
            .renderer(add_input_panel)
            .on_submit({
                let link = ctx.link().clone();
                move |form_ctx| {
                    let link = link.clone();
                    async move {
                        match create_token(form_ctx).await {
                            Ok(AnswerTokenCreateResult { token, secret }) => {
                                link.change_view(Some(ViewState::DisplaySecret { token, secret }));
                                Ok(())
                            }
                            Err(err) => Err(err),
                        }
                    }
                }
            })
            .on_close(ctx.link().change_view_callback(|_| None))
            .into();

        Some(window)
    }

    fn create_edit_dialog(&self, ctx: &LoadableComponentContext<Self>) -> Option<yew::Html> {
        let record = self
            .store
            .read()
            .lookup_record(&self.selection.selected_key()?)?
            .clone();

        let window = EditWindow::new(tr!("Edit") + ": " + &tr!("Token"))
            // dirty-gate the Update button and show a Reset button, matching the prepared-answer
            // edit flow (the form is seeded from the record, not loaded asynchronously).
            .edit(true)
            .renderer({
                let record = record.clone();
                move |form_ctx: &FormContext| edit_input_panel(form_ctx, &record)
            })
            .submit_text(tr!("Update"))
            .on_submit({
                let id = record.id.clone();
                move |form_ctx| {
                    let id = id.clone();
                    async move { update_token(form_ctx, &id).await }
                }
            })
            .on_done(ctx.link().change_view_callback(|_| None))
            .into();

        Some(window)
    }
}

fn columns() -> Vec<DataTableHeader<AnswerToken>> {
    vec![
        DataTableColumn::new(tr!("Name"))
            .width("200px")
            .render(|item: &AnswerToken| html! { &item.id })
            .sorter(|a: &AnswerToken, b: &AnswerToken| a.id.cmp(&b.id))
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Created By"))
            .width("150px")
            .render(|item: &AnswerToken| html! { &item.created_by })
            .sorter(|a: &AnswerToken, b: &AnswerToken| a.created_by.cmp(&b.created_by))
            .into(),
        DataTableColumn::new(tr!("Enabled"))
            .width("80px")
            .render(|item: &AnswerToken| {
                if item.enabled.unwrap_or(false) {
                    Fa::new("check").into()
                } else {
                    Fa::new("times").into()
                }
            })
            .sorter(|a: &AnswerToken, b: &AnswerToken| a.enabled.cmp(&b.enabled))
            .into(),
        DataTableColumn::new(tr!("Expire"))
            .width("200px")
            .render({
                move |item: &AnswerToken| {
                    html! {
                        match item.expire_at {
                            Some(epoch) if epoch != 0 => render_epoch(epoch),
                            _ => tr!("never"),
                        }
                    }
                }
            })
            .sorter(|a: &AnswerToken, b: &AnswerToken| {
                let a = a
                    .expire_at
                    .and_then(|exp| if exp == 0 { None } else { Some(exp) });
                let b = b
                    .expire_at
                    .and_then(|exp| if exp == 0 { None } else { Some(exp) });

                a.cmp(&b)
            })
            .into(),
        DataTableColumn::new("Comment")
            .flex(1)
            .render(|item: &AnswerToken| html! { item.comment.clone().unwrap_or_default() })
            .into(),
    ]
}

fn edit_input_panel(form_ctx: &FormContext, token: &AnswerToken) -> Html {
    let mut panel = InputPanel::new().padding(4);
    panel.add_field(
        tr!("Token Name"),
        Field::new()
            .name("id")
            .value(token.id.clone())
            .submit(false)
            .disabled(true)
            .required(true),
    );
    add_expiry_fields(&mut panel, form_ctx, token.expire_at);
    panel.add_right_field(
        tr!("Enabled"),
        Checkbox::new().name("enabled").default(token.enabled),
    );
    panel.add_large_field(
        false,
        false,
        tr!("Comment"),
        Field::new()
            .name("comment")
            .value(token.comment.clone())
            .default(token.comment.clone())
            .submit_empty(true),
    );
    panel.into()
}

fn add_input_panel(form_ctx: &FormContext) -> Html {
    let mut panel = InputPanel::new().padding(4);
    panel.add_field(
        tr!("Token Name"),
        Field::new().name("id").submit(false).required(true),
    );
    add_expiry_fields(&mut panel, form_ctx, None);
    panel.add_right_field(
        tr!("Enabled"),
        Checkbox::new().name("enabled").default(true),
    );
    panel.add_large_field(false, false, tr!("Comment"), Field::new().name("comment"));
    panel.into()
}

/// Adds the token expiry controls: an "Expires" checkbox plus a date-time field that is only active
/// while it is set. An unset checkbox means the token never expires and is also how an existing
/// expiry is cleared. The date-time field is required while active, so a half-entered value (which
/// the native input reports as empty) is rejected here instead of being silently stored as never.
fn add_expiry_fields(panel: &mut InputPanel, form_ctx: &FormContext, expire_at: Option<i64>) {
    let has_expiry = matches!(expire_at, Some(exp) if exp != 0);
    let active = form_ctx.read().get_field_checked("has-expiry");
    // epoch_to_input_value yields the `YYYY-MM-DDTHH:MM` form the datetime-local input
    // needs; seed it as the field default too so an unchanged edit stays non-dirty.
    let value = expire_at.and_then(|exp| (exp != 0).then(|| epoch_to_input_value(exp)));
    panel.add_right_field(
        tr!("Expires"),
        Checkbox::new()
            .name("has-expiry")
            .submit(false)
            .default(has_expiry),
    );
    panel.add_right_field(
        tr!("Expire"),
        Field::new()
            .name("expire-at")
            .disabled(!active)
            .required(active)
            .default(value.clone())
            .value(value)
            .input_type(InputType::DatetimeLocal)
            .validate(|s: &String| {
                match proxmox_time::parse_rfc3339(s) {
                    Ok(t) if t <= proxmox_time::epoch_i64() => {
                        bail!(tr!("expiry must be in the future"))
                    }
                    _ => Ok(()),
                }
            }),
    );
}

/// Reads the expiry as a UNIX epoch, or 0 (never) when the "Expires" checkbox is unset.
fn read_expiry(form_ctx: &FormContext) -> i64 {
    if form_ctx.read().get_field_checked("has-expiry") {
        proxmox_time::parse_rfc3339(&form_ctx.read().get_field_text("expire-at")).unwrap_or(0)
    } else {
        0
    }
}

async fn create_token(form_ctx: FormContext) -> Result<AnswerTokenCreateResult> {
    let id = form_ctx.read().get_field_text("id");
    let comment = form_ctx.read().get_field_text("comment");
    let enable = form_ctx.read().get_field_checked("enabled");
    let expire = read_expiry(&form_ctx);

    Ok(pdm_client()
        .add_autoinst_token(&id, Some(comment), Some(enable), Some(expire))
        .await?)
}

async fn update_token(form_ctx: FormContext, id: &str) -> Result<()> {
    let updater = AnswerTokenUpdater {
        comment: Some(form_ctx.read().get_field_text("comment")),
        enabled: Some(form_ctx.read().get_field_checked("enabled")),
        expire_at: Some(read_expiry(&form_ctx)),
    };

    pdm_client()
        .update_autoinst_token(&percent_encode_component(id), &updater, &[], false)
        .await?;
    Ok(())
}

async fn regenerate_token_secret(id: &str) -> Result<AnswerTokenUpdateResult> {
    Ok(pdm_client()
        .update_autoinst_token(
            &percent_encode_component(id),
            &AnswerTokenUpdater::default(),
            &[],
            true,
        )
        .await?)
}
