use std::{collections::HashSet, rc::Rc};

use anyhow::{bail, format_err, Error};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use yew::{virtual_dom::Key, Callback, Component, Html, Properties};

use pdm_client::types::{CreateVnetParams, ListZone, SDN_ID_SCHEMA};
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

#[widget(comp=AddVnetWindowComp)]
#[derive(Properties, PartialEq, Clone)]
pub struct AddVnetWindow {
    pub zones: Rc<Vec<ListZone>>,
    pub on_success: Option<Callback<String>>,
    pub on_close: Option<Callback<()>>,
}

impl AddVnetWindow {
    pub fn new(
        zones: Rc<Vec<ListZone>>,
        on_success: impl Into<Option<Callback<String>>>,
        on_close: impl Into<Option<Callback<()>>>,
    ) -> Self {
        yew::props!(Self {
            zones,
            on_success: on_success.into(),
            on_close: on_close.into(),
        })
    }
}

pub struct AddVnetWindowComp {}

impl Component for AddVnetWindowComp {
    type Message = ();

    type Properties = AddVnetWindow;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self {}
    }

    fn view(&self, ctx: &yew::Context<Self>) -> Html {
        let props = ctx.props().clone();

        EditWindow::new(tr!("Add VNet"))
            .renderer(move |form_ctx: &FormContext| {
                InputPanel::new()
                    .class(css::FlexFit)
                    .padding(4)
                    .width("auto")
                    .with_field(
                        tr!("VNet ID"),
                        Field::new()
                            .name("vnet")
                            .schema(&SDN_ID_SCHEMA)
                            .required(true),
                    )
                    .with_field(
                        tr!("VXLAN VNI"),
                        Number::<u32>::new()
                            .min(1)
                            .max(16777215)
                            .name("tag")
                            .required(true),
                    )
                    .with_custom_child(
                        Column::new()
                            .with_child(ZoneTable::new(props.zones.clone()).name("remotes"))
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

                        let params: CreateVnetParams =
                            serde_json::from_value(form_ctx.get_submit_data()).unwrap();

                        let upid = client.pve_sdn_create_vnet(params).await?;

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

#[widget(comp=ManagedFieldMaster<ZoneTableComponent>, @input)]
#[derive(Clone, PartialEq, Properties)]
pub struct ZoneTable {
    zones: Rc<Vec<ListZone>>,
}

impl ZoneTable {
    pub fn new(zones: Rc<Vec<ListZone>>) -> Self {
        yew::props!(Self { zones })
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ZoneTableEntry {
    remote: String,
    zone: String,
    vni: u32,
}

impl ExtractPrimaryKey for ZoneTableEntry {
    fn extract_key(&self) -> Key {
        Key::from(format!("{}/{}", self.remote, self.zone))
    }
}

pub struct ZoneTableComponent {
    store: Store<ZoneTableEntry>,
    selection: Selection,
    columns: Rc<Vec<DataTableHeader<ZoneTableEntry>>>,
    error_msg: Option<String>,
}

pub enum ZoneTableMsg {
    SelectionChange,
}

#[derive(PartialEq)]
pub struct ValidationContext {
    zone_count: usize,
}

impl ManagedField for ZoneTableComponent {
    type Properties = ZoneTable;
    type Message = ZoneTableMsg;
    type ValidateClosure = ValidationContext;

    fn validation_args(props: &Self::Properties) -> Self::ValidateClosure {
        ValidationContext {
            zone_count: props.zones.len(),
        }
    }

    fn validator(props: &Self::ValidateClosure, value: &Value) -> Result<Value, Error> {
        let selected_entries: Vec<ZoneTableEntry> = serde_json::from_value(value.clone())?;

        if selected_entries.is_empty() {
            if props.zone_count == 0 {
                bail!(tr!("At least one zone needs to be configured on a remote"));
            } else {
                bail!(tr!("At least one zone needs to be selected"));
            }
        }

        let mut unique = HashSet::new();

        if !selected_entries
            .iter()
            .all(|entry| unique.insert(entry.remote.as_str()))
        {
            bail!(tr!("Can only create the VNet once per remote!"))
        }

        Ok(value.clone())
    }

    fn setup(_props: &Self::Properties) -> ManagedFieldState {
        ManagedFieldState {
            value: Value::Array(Vec::new()),
            valid: Ok(()),
            default: Value::Array(Vec::new()),
            radio_group: false,
            unique: false,
        }
    }

    fn create(ctx: &ManagedFieldContext<Self>) -> Self {
        let columns = Self::columns();
        let link = ctx.link().clone();
        let selection = Selection::new().multiselect(true).on_select(move |_| {
            link.send_message(Self::Message::SelectionChange);
        });

        let store = Store::new();

        let zones: Result<Vec<ZoneTableEntry>, Error> =
            ctx.props()
                .zones
                .iter()
                .map(|zone| {
                    Ok(ZoneTableEntry {
                        remote: zone.remote.clone(),
                        zone: zone.zone.zone.clone(),
                        vni: zone.zone.vrf_vxlan.ok_or_else(|| {
                            format_err!(tr!("EVPN Zone has no VRF VNI configured!"))
                        })?,
                    })
                })
                .collect();

        let mut error_msg = None;

        match zones {
            Ok(zones) => {
                store.set_data(zones);
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

impl ZoneTableComponent {
    fn columns() -> Rc<Vec<DataTableHeader<ZoneTableEntry>>> {
        Rc::new(vec![
            DataTableColumn::selection_indicator().into(),
            DataTableColumn::new(tr!("Remote"))
                .flex(1)
                .render(move |item: &ZoneTableEntry| item.remote.as_str().into())
                .sorter(|a: &ZoneTableEntry, b: &ZoneTableEntry| a.remote.cmp(&b.remote))
                .into(),
            DataTableColumn::new(tr!("Zone"))
                .flex(1)
                .render(move |item: &ZoneTableEntry| item.zone.as_str().into())
                .sorter(|a: &ZoneTableEntry, b: &ZoneTableEntry| a.zone.cmp(&b.zone))
                .into(),
            DataTableColumn::new(tr!("VRF VNI"))
                .flex(1)
                .render(move |item: &ZoneTableEntry| item.vni.to_string().into())
                .sorter(|a: &ZoneTableEntry, b: &ZoneTableEntry| a.vni.cmp(&b.vni))
                .into(),
        ])
    }
}
