use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::{bail, Error};

use pdm_api_types::VIEW_ID_SCHEMA;
use yew::virtual_dom::{Key, VComp, VNode};

use proxmox_yew_comp::form::delete_empty_values;
use proxmox_yew_comp::percent_encoding::percent_encode_component;
use proxmox_yew_comp::{http_delete, http_get, http_post, http_put, EditWindow, SchemaValidation};
use proxmox_yew_comp::{
    LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState,
};

use pwt::prelude::*;
use pwt::state::{Selection, Store};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::{Checkbox, DisplayField, Field, FormContext};
use pwt::widget::{Button, ConfirmDialog, InputPanel, Toolbar};

use pdm_api_types::views::{ViewConfig, ViewLayout, ViewTemplate};

use crate::widget::{ViewFilterSelector, ViewSelector};
use crate::ViewListContext;

async fn create_view(
    base_url: AttrValue,
    store: Store<ViewConfig>,
    form_ctx: FormContext,
) -> Result<(), Error> {
    let mut data = form_ctx.get_submit_data();
    let layout = form_ctx.read().get_field_text("copy-from");
    let layout = match layout.as_str() {
        "" => Some(serde_json::to_string(&ViewTemplate {
            description: String::new(),
            layout: ViewLayout::Rows { rows: Vec::new() },
        })?),
        "__dashboard__" => None,
        layout => {
            let store = store.read();
            if let Some(config) = store.lookup_record(&Key::from(layout)) {
                Some(config.layout.clone())
            } else {
                bail!("Source View not found")
            }
        }
    };

    if let Some(layout) = layout {
        data["layout"] = layout.into();
    }

    let config: ViewConfig = serde_json::from_value(data)?;

    http_post(base_url.as_str(), Some(serde_json::to_value(config)?)).await
}

async fn update_view(base_url: AttrValue, form_ctx: FormContext) -> Result<(), Error> {
    let data = form_ctx.get_submit_data();
    let id = form_ctx.read().get_field_text("id");
    let params = delete_empty_values(&data, &["include", "exclude", "include-all"], true);
    let id = percent_encode_component(&id);

    http_put(&format!("{base_url}/{id}"), Some(params)).await
}

#[derive(PartialEq, Clone, Properties)]
pub struct ViewGrid {
    #[prop_or("/config/views".into())]
    base_url: AttrValue,
}

impl ViewGrid {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

impl Default for ViewGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl From<ViewGrid> for VNode {
    fn from(val: ViewGrid) -> Self {
        VComp::new::<LoadableComponentMaster<ViewGridComp>>(Rc::new(val), None).into()
    }
}

pub enum Msg {
    LoadFinished(Vec<ViewConfig>),
    Remove(Key),
    Reload,
}

#[derive(PartialEq)]
pub enum ViewState {
    Create,
    Edit,
    Remove,
}

#[doc(hidden)]
pub struct ViewGridComp {
    state: LoadableComponentState<ViewState>,
    store: Store<ViewConfig>,
    columns: Rc<Vec<DataTableHeader<ViewConfig>>>,
    selection: Selection,
}

pwt::impl_deref_mut_property!(ViewGridComp, state, LoadableComponentState<ViewState>);

impl ViewGridComp {
    fn columns() -> Rc<Vec<DataTableHeader<ViewConfig>>> {
        let columns = vec![
            DataTableColumn::new("ID")
                .flex(5)
                .get_property(|value: &ViewConfig| value.id.as_str())
                .sort_order(true)
                .into(),
            DataTableColumn::new(tr!("# Included"))
                .flex(1)
                .sorter(|a: &ViewConfig, b: &ViewConfig| {
                    let a = if a.include_all.unwrap_or_default() {
                        usize::MAX
                    } else {
                        a.include.len()
                    };
                    let b = if b.include_all.unwrap_or_default() {
                        usize::MAX
                    } else {
                        b.include.len()
                    };
                    a.cmp(&b)
                })
                .render(|value: &ViewConfig| {
                    if value.include_all.unwrap_or_default() {
                        tr!("All").into()
                    } else {
                        value.include.len().into()
                    }
                })
                .into(),
            DataTableColumn::new(tr!("# Excluded"))
                .flex(1)
                .get_property_owned(|value: &ViewConfig| value.exclude.len())
                .into(),
            DataTableColumn::new(tr!("Custom Layout"))
                .flex(1)
                .render(|value: &ViewConfig| {
                    if value.layout.is_empty() {
                        tr!("No").into()
                    } else {
                        tr!("Yes").into()
                    }
                })
                .into(),
        ];

        Rc::new(columns)
    }

    fn create_add_dialog(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        let props = ctx.props();
        let store = self.store.clone();
        EditWindow::new(tr!("Add") + ": " + &tr!("View"))
            .renderer(move |form_ctx| input_panel(form_ctx, InputPanelMode::Create(store.clone())))
            .on_submit({
                let base_url = props.base_url.clone();
                let store = self.store.clone();
                move |form| create_view(base_url.clone(), store.clone(), form)
            })
            .on_done(ctx.link().clone().callback(|_| Msg::Reload))
            .into()
    }

    fn create_edit_dialog(&self, selection: Key, ctx: &LoadableComponentContext<Self>) -> Html {
        let props = ctx.props();
        let id = selection.to_string();
        EditWindow::new(tr!("Edit") + ": " + &tr!("View"))
            .renderer(move |form_ctx| input_panel(form_ctx, InputPanelMode::Edit(id.clone())))
            .on_submit({
                let base_url = props.base_url.clone();
                move |form| update_view(base_url.clone(), form)
            })
            .loader(format!(
                "{}/{}",
                props.base_url,
                percent_encode_component(&selection)
            ))
            .on_done(ctx.link().callback(|_| Msg::Reload))
            .into()
    }
}

impl LoadableComponent for ViewGridComp {
    type Properties = ViewGrid;
    type Message = Msg;
    type ViewState = ViewState;

    fn create(ctx: &proxmox_yew_comp::LoadableComponentContext<Self>) -> Self {
        let selection = Selection::new().on_select({
            let link = ctx.link().clone();
            move |_| link.send_redraw()
        });
        Self {
            state: LoadableComponentState::new(),
            store: Store::with_extract_key(|config: &ViewConfig| config.id.as_str().into()),
            columns: Self::columns(),
            selection,
        }
    }

    fn update(
        &mut self,
        ctx: &proxmox_yew_comp::LoadableComponentContext<Self>,
        msg: Self::Message,
    ) -> bool {
        match msg {
            Msg::LoadFinished(data) => self.store.set_data(data),
            Msg::Remove(key) => {
                if let Some(rec) = self.store.read().lookup_record(&key) {
                    let id = rec.id.clone();
                    let link = ctx.link().clone();
                    let base_url = ctx.props().base_url.clone();
                    ctx.link().spawn(async move {
                        match http_delete(format!("{base_url}/{id}"), None).await {
                            Ok(()) => {}
                            Err(err) => {
                                link.show_error(
                                    tr!("Error"),
                                    tr!("Could not delete '{0}': '{1}'", id, err),
                                    true,
                                );
                            }
                        }
                        link.send_message(Msg::Reload);
                    });
                }
            }
            Msg::Reload => {
                ctx.link().change_view(None);
                ctx.link().send_reload();
                if let Some((context, _)) = ctx
                    .link()
                    .context::<ViewListContext>(Callback::from(|_| {}))
                {
                    context.update_views();
                }
            }
        }
        true
    }

    fn toolbar(&self, ctx: &proxmox_yew_comp::LoadableComponentContext<Self>) -> Option<Html> {
        let selection = self.selection.selected_key();
        let link = ctx.link();
        Some(
            Toolbar::new()
                .border_bottom(true)
                .with_child(
                    Button::new(tr!("Add"))
                        .on_activate(link.change_view_callback(|_| Some(ViewState::Create))),
                )
                .with_child(
                    Button::new(tr!("Edit"))
                        .disabled(selection.is_none())
                        .on_activate(link.change_view_callback(move |_| Some(ViewState::Edit))),
                )
                .with_child(
                    Button::new(tr!("Remove"))
                        .disabled(selection.is_none())
                        .on_activate(link.change_view_callback(move |_| Some(ViewState::Remove))),
                )
                .into(),
        )
    }

    fn load(
        &self,
        ctx: &proxmox_yew_comp::LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>>>> {
        let base_url = ctx.props().base_url.clone();
        let link = ctx.link().clone();
        Box::pin(async move {
            let data: Vec<ViewConfig> = http_get(base_url.as_str(), None).await?;
            link.send_message(Msg::LoadFinished(data));
            Ok(())
        })
    }

    fn main_view(&self, ctx: &proxmox_yew_comp::LoadableComponentContext<Self>) -> Html {
        let link = ctx.link().clone();
        DataTable::new(self.columns.clone(), self.store.clone())
            .on_row_dblclick(move |_: &mut _| link.change_view(Some(ViewState::Edit)))
            .selection(self.selection.clone())
            .into()
    }

    fn dialog_view(
        &self,
        ctx: &proxmox_yew_comp::LoadableComponentContext<Self>,
        view_state: &Self::ViewState,
    ) -> Option<Html> {
        match view_state {
            ViewState::Create => Some(self.create_add_dialog(ctx)),
            ViewState::Edit => self
                .selection
                .selected_key()
                .map(|key| self.create_edit_dialog(key, ctx)),
            ViewState::Remove => self.selection.selected_key().map(|key| {
                ConfirmDialog::new(
                    tr!("Confirm"),
                    tr!("Are you sure you want to remove '{0}'", key.to_string()),
                )
                .on_confirm({
                    let link = ctx.link().clone();
                    let key = key.clone();
                    move |_| {
                        link.send_message(Msg::Remove(key.clone()));
                    }
                })
                .into()
            }),
        }
    }
}

enum InputPanelMode {
    Create(Store<ViewConfig>),
    Edit(String), // id
}

fn input_panel(form_ctx: &FormContext, mode: InputPanelMode) -> Html {
    let include_all = form_ctx.read().get_field_checked("include-all");
    let is_create = matches!(mode, InputPanelMode::Create(_));
    let mut input_panel = InputPanel::new().padding(4);

    match mode {
        InputPanelMode::Create(store) => {
            input_panel.add_field(
                tr!("Name"),
                Field::new()
                    .name("id")
                    .schema(&VIEW_ID_SCHEMA)
                    .required(true),
            );
            input_panel.add_right_field(
                tr!("Copy Layout from"),
                ViewSelector::new(store)
                    .placeholder(tr!("None"))
                    .name("copy-from"),
            );
        }
        InputPanelMode::Edit(id) => input_panel.add_field(
            tr!("Name"),
            DisplayField::new().name("id").value(id.clone()),
        ),
    }

    input_panel
        .with_large_field(
            tr!("Include All"),
            Checkbox::new()
                .name("include-all")
                .box_label(tr!("Include all remotes and their resources."))
                .default(is_create),
        )
        .with_field_and_options(
            pwt::widget::FieldPosition::Large,
            false,
            include_all,
            tr!("Include"),
            ViewFilterSelector::new()
                .name("include")
                .disabled(include_all),
        )
        .with_large_field(tr!("Exclude"), ViewFilterSelector::new().name("exclude"))
        .into()
}
