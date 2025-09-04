use std::{collections::HashSet, rc::Rc};

use anyhow::{bail, format_err, Error};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use yew::{virtual_dom::Key, Callback, Component, Html, Properties};

use pdm_client::types::{CreateZoneParams, ListController, SDN_ID_SCHEMA};
use proxmox_yew_comp::{EditWindow, SchemaValidation};
use pwt::{
    css,
    props::{
        ContainerBuilder, CssBorderBuilder, CssPaddingBuilder, ExtractPrimaryKey, FieldBuilder,
        WidgetBuilder, WidgetStyleBuilder,
    },
    state::{Selection, Store},
    tr,
    widget::{
        data_table::{DataTable, DataTableColumn, DataTableHeader, MultiSelectMode},
        error_message,
        form::{
            Field, FormContext, ManagedField, ManagedFieldContext, ManagedFieldMaster,
            ManagedFieldState, Number,
        },
        Column, Container, GridPicker, InputPanel,
    },
};
use pwt_macros::widget;

use crate::pdm_client;

#[widget(comp=AddZoneWindowComp)]
#[derive(Properties, PartialEq, Clone)]
pub struct AddZoneWindow {
    pub controllers: Rc<Vec<ListController>>,
    pub on_success: Option<Callback<String>>,
    pub on_close: Option<Callback<()>>,
}

impl AddZoneWindow {
    pub fn new(
        controllers: Rc<Vec<ListController>>,
        on_success: impl Into<Option<Callback<String>>>,
        on_close: impl Into<Option<Callback<()>>>,
    ) -> Self {
        yew::props!(Self {
            controllers,
            on_success: on_success.into(),
            on_close: on_close.into(),
        })
    }
}

pub struct AddZoneWindowComp {}

impl Component for AddZoneWindowComp {
    type Message = ();

    type Properties = AddZoneWindow;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self {}
    }

    fn view(&self, ctx: &yew::Context<Self>) -> Html {
        let props = ctx.props().clone();

        EditWindow::new(tr!("Add Zone"))
            .renderer(move |form_ctx: &FormContext| {
                InputPanel::new()
                    .class(css::FlexFit)
                    .padding(4)
                    .width("auto")
                    .with_field(
                        tr!("Zone ID"),
                        Field::new()
                            .name("zone")
                            .schema(&SDN_ID_SCHEMA)
                            .required(true),
                    )
                    .with_field(
                        tr!("VRF VXLAN VNI"),
                        Number::<u32>::new()
                            .min(1)
                            .max(16777215)
                            .name("vrf-vxlan")
                            .required(true),
                    )
                    .with_custom_child(
                        Column::new()
                            .with_child(
                                ControllerTable::new(props.controllers.clone()).name("remotes"),
                            )
                            .with_optional_child(
                                form_ctx
                                    .read()
                                    .get_field_valid("remotes")
                                    .and_then(|result| result.err().as_deref().map(error_message)),
                            ),
                    )
                    .into()
            })
            .on_close(ctx.props().on_close.clone())
            .on_submit({
                let on_success = props.on_success.clone();

                move |form_ctx: FormContext| {
                    let on_success = on_success.clone();

                    async move {
                        let client = pdm_client();

                        let params: CreateZoneParams =
                            serde_json::from_value(form_ctx.get_submit_data()).unwrap();

                        let upid = client.pve_sdn_create_zone(params).await?;

                        if let Some(cb) = on_success {
                            cb.emit(upid)
                        }

                        Ok(())
                    }
                }
            })
            .into()
    }
}

#[widget(comp=ManagedFieldMaster<ControllerTableComponent>, @input)]
#[derive(Clone, PartialEq, Properties)]
struct ControllerTable {
    controllers: Rc<Vec<ListController>>,
}

impl ControllerTable {
    pub fn new(controllers: Rc<Vec<ListController>>) -> Self {
        yew::props!(Self { controllers })
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
struct ControllerTableEntry {
    remote: String,
    controller: String,
    #[serde(skip)]
    asn: u32,
}

impl ExtractPrimaryKey for ControllerTableEntry {
    fn extract_key(&self) -> Key {
        Key::from(format!("{}/{}", self.remote, self.controller))
    }
}

struct ControllerTableComponent {
    store: Store<ControllerTableEntry>,
    selection: Selection,
    columns: Rc<Vec<DataTableHeader<ControllerTableEntry>>>,
    error_msg: Option<String>,
}

enum ControllerTableMsg {
    SelectionChange,
}

#[derive(PartialEq)]
struct ValidationContext {
    controller_count: usize,
}

impl ManagedField for ControllerTableComponent {
    type Properties = ControllerTable;
    type Message = ControllerTableMsg;
    type ValidateClosure = ValidationContext;

    fn validation_args(props: &Self::Properties) -> Self::ValidateClosure {
        ValidationContext {
            controller_count: props.controllers.len(),
        }
    }

    fn validator(props: &Self::ValidateClosure, value: &Value) -> Result<Value, Error> {
        let selected_entries: Vec<ControllerTableEntry> = serde_json::from_value(value.clone())?;

        if selected_entries.is_empty() {
            if props.controller_count == 0 {
                bail!(tr!(
                    "at least one remote needs to have an EVPN controller configured"
                ));
            } else {
                bail!(tr!("at least one EVPN controller needs to be selected"));
            }
        }

        let mut unique = HashSet::new();

        if !selected_entries
            .iter()
            .all(|entry| unique.insert(entry.remote.as_str()))
        {
            bail!(tr!("can only create the zone once per remote!"));
        }

        Ok(value.clone())
    }

    fn setup(_props: &Self::Properties) -> ManagedFieldState {
        ManagedFieldState {
            value: Value::Null,
            valid: Ok(()),
            default: Value::Array(Vec::new()),
            radio_group: false,
            unique: false,
        }
    }

    fn create(ctx: &ManagedFieldContext<Self>) -> Self {
        let link = ctx.link().clone();

        let selection = Selection::new().multiselect(true).on_select(move |_| {
            link.send_message(Self::Message::SelectionChange);
        });

        let store = Store::new();

        let columns = Self::columns();

        let controllers: Result<Vec<ControllerTableEntry>, Error> = ctx
            .props()
            .controllers
            .iter()
            .map(|controller| {
                Ok(ControllerTableEntry {
                    remote: controller.remote.clone(),
                    controller: controller.controller.controller.clone(),
                    asn: controller.controller.asn.ok_or_else(|| {
                        format_err!(tr!(
                            "EVPN controller {} has no ASN",
                            controller.controller.controller
                        ))
                    })?,
                })
            })
            .collect();

        let mut error_msg = None;

        match controllers {
            Ok(controllers) => {
                store.set_data(controllers);
            }
            Err(error) => error_msg = Some(error.to_string()),
        };

        Self {
            store,
            selection,
            columns,
            error_msg,
        }
    }

    fn update(&mut self, ctx: &ManagedFieldContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Self::Message::SelectionChange => {
                let read_guard = self.store.read();

                ctx.link().update_value(
                    serde_json::to_value(
                        self.selection
                            .selected_keys()
                            .iter()
                            // todo: handle miss?
                            .filter_map(|key| read_guard.lookup_record(key))
                            .collect::<Vec<_>>(),
                    )
                    .unwrap(),
                );
            }
        }

        false
    }

    fn view(&self, _ctx: &ManagedFieldContext<Self>) -> Html {
        let table = DataTable::new(self.columns.clone(), self.store.clone())
            .multiselect_mode(MultiSelectMode::Simple)
            .border(true)
            .class(css::FlexFit);

        let mut container =
            Container::new().with_child(GridPicker::new(table).selection(self.selection.clone()));

        if let Some(error_msg) = &self.error_msg {
            container.add_child(error_message(error_msg));
        }

        container.into()
    }
}

impl ControllerTableComponent {
    fn columns() -> Rc<Vec<DataTableHeader<ControllerTableEntry>>> {
        Rc::new(vec![
            DataTableColumn::selection_indicator().into(),
            DataTableColumn::new(tr!("Remote"))
                .flex(1)
                .render(move |item: &ControllerTableEntry| item.remote.as_str().into())
                .sorter(|a: &ControllerTableEntry, b: &ControllerTableEntry| {
                    a.remote.cmp(&b.remote)
                })
                .into(),
            DataTableColumn::new(tr!("Controller"))
                .flex(1)
                .render(move |item: &ControllerTableEntry| item.controller.as_str().into())
                .sorter(|a: &ControllerTableEntry, b: &ControllerTableEntry| {
                    a.controller.cmp(&b.controller)
                })
                .into(),
            DataTableColumn::new(tr!("ASN"))
                .flex(1)
                .render(move |item: &ControllerTableEntry| item.asn.into())
                .sorter(|a: &ControllerTableEntry, b: &ControllerTableEntry| a.asn.cmp(&b.asn))
                .into(),
        ])
    }
}
