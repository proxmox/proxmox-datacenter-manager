use std::rc::Rc;

use anyhow::Error;
use serde_json::Value;

use yew::virtual_dom::Key;

use pwt::state::Store;
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::ManagedField;
use pwt::widget::form::{Field, ManagedFieldContext, ManagedFieldMaster, ManagedFieldState};
use pwt::widget::{ActionIcon, Button, Column, Row};
use pwt::{css, prelude::*};

use proxmox_yew_comp::SchemaValidation;

use pdm_api_types::remotes::NodeUrl;
use proxmox_schema::property_string::PropertyString;

use pbs_api_types::CERT_FINGERPRINT_SHA256_SCHEMA;

use pwt_macros::{builder, widget};

//pub type PdmNodeUrlList = ManagedFieldMaster<PdmNodeUrlField>;

#[widget(comp=ManagedFieldMaster<PdmNodeUrlField>, @input)]
#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct NodeUrlList {}

impl NodeUrlList {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    index: usize,
    data: NodeUrl,
}

#[doc(hidden)]
pub struct PdmNodeUrlField {
    store: Store<Entry>,
    columns: Rc<Vec<DataTableHeader<Entry>>>,
}

pub enum Msg {
    DataChange,
    UpdateHostname(usize, String),
    UpdateFingerprint(usize, String),
    RemoveUrl(usize),
}

impl PdmNodeUrlField {
    fn set_nodes(&mut self, nodes: Vec<NodeUrl>) {
        self.store.set_data(
            nodes
                .into_iter()
                .enumerate()
                .map(|(index, data)| Entry { index, data })
                .collect(),
        );
    }

    pub fn sync_from_value(&mut self, value: Value) {
        match serde_json::from_value::<Vec<PropertyString<NodeUrl>>>(value) {
            Ok(items) => self.set_nodes(items.into_iter().map(|item| item.into_inner()).collect()),
            Err(_err) => {
                // unable to parse node list, likely caused by the user editing items.
                // simply ignore errors
                return;
            }
        }
    }
}

impl ManagedField for PdmNodeUrlField {
    type Message = Msg;
    type Properties = NodeUrlList;
    type ValidateClosure = ();

    fn validation_args(_props: &Self::Properties) -> Self::ValidateClosure {
        ()
    }

    fn validator(_props: &Self::ValidateClosure, value: &Value) -> Result<Value, Error> {
        serde_json::from_value::<Vec<PropertyString<NodeUrl>>>(value.clone())?;
        Ok(value.clone())
    }

    fn setup(_props: &Self::Properties) -> ManagedFieldState {
        let value = Value::Null;
        let default = Value::Null;

        let valid = Ok(());

        ManagedFieldState {
            value,
            valid,
            default,
            radio_group: false,
            unique: false,
        }
    }

    fn create(ctx: &pwt::widget::form::ManagedFieldContext<Self>) -> Self {
        let store = Store::with_extract_key(|entry: &Entry| Key::from(entry.index))
            .on_change(ctx.link().callback(|_| Msg::DataChange));
        let columns = columns(ctx);

        Self { store, columns }
    }
    fn value_changed(&mut self, ctx: &ManagedFieldContext<Self>) {
        let state = ctx.state();
        self.sync_from_value(state.value.clone());
    }

    fn update(&mut self, ctx: &ManagedFieldContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::DataChange => {
                let list: Vec<PropertyString<NodeUrl>> = self
                    .store
                    .read()
                    .iter()
                    .map(|item| PropertyString::new(item.data.clone()))
                    .collect();
                ctx.link().update_value(serde_json::to_value(list).unwrap());
                true
            }
            Msg::RemoveUrl(index) => {
                let data: Vec<NodeUrl> = self
                    .store
                    .read()
                    .iter()
                    .filter(move |item| item.index != index)
                    .map(|item| item.data.clone())
                    .collect();
                self.set_nodes(data);
                true
            }
            Msg::UpdateHostname(index, hostname) => {
                let mut data = self.store.write();
                if let Some(item) = data.get_mut(index) {
                    item.data.hostname = hostname;
                }
                true
            }
            Msg::UpdateFingerprint(index, fingerprint) => {
                let mut data = self.store.write();
                if let Some(item) = data.get_mut(index) {
                    if fingerprint.is_empty() {
                        item.data.fingerprint = None;
                    } else {
                        item.data.fingerprint = Some(fingerprint);
                    }
                }
                true
            }
        }
    }

    fn view(&self, _ctx: &pwt::widget::form::ManagedFieldContext<Self>) -> Html {
        let table = DataTable::new(self.columns.clone(), self.store.clone())
            .border(true)
            .class(css::FlexFit);

        let toolbar = Row::new().with_child(
            Button::new(tr!("Add"))
                .class(css::ColorScheme::Primary)
                .icon_class("fa fa-plus-circle")
                .onclick({
                    let nodes = self.store.clone();
                    move |_| {
                        let mut nodes = nodes.write();
                        let index = nodes.len();

                        nodes.push(Entry {
                            index,
                            data: NodeUrl {
                                hostname: String::new(),
                                fingerprint: None,
                            },
                        })
                    }
                }),
        );

        Column::new()
            .height(200)
            .gap(2)
            .with_child(table)
            .with_child(toolbar)
            .into()
    }
}

fn columns(ctx: &ManagedFieldContext<PdmNodeUrlField>) -> Rc<Vec<DataTableHeader<Entry>>> {
    let link = ctx.link();

    Rc::new(vec![
        DataTableColumn::new(tr!("Hostname"))
            .flex(1)
            .render({
                let link = link.clone();
                move |item: &Entry| {
                    let index = item.index;
                    Field::new()
                        .on_change(link.callback(move |value| Msg::UpdateHostname(index, value)))
                        .required(true)
                        .value(item.data.hostname.clone())
                        .into()
                }
            })
            .sorter(|a: &Entry, b: &Entry| a.data.hostname.cmp(&b.data.hostname))
            .sort_order(None)
            .into(),
        DataTableColumn::new(tr!("Fingerprint"))
            .flex(2)
            .render({
                let link = link.clone();
                move |item: &Entry| {
                    let index = item.index;
                    let fingerprint = match &item.data.fingerprint {
                        Some(fingerprint) => fingerprint,
                        None => "",
                    };
                    Field::new()
                        .schema(&CERT_FINGERPRINT_SHA256_SCHEMA)
                        .on_change(link.callback(move |value| Msg::UpdateFingerprint(index, value)))
                        .value(fingerprint.to_string())
                        .into()
                }
            })
            .into(),
        DataTableColumn::new("")
            .width("50px")
            .render(move |item: &Entry| {
                let index = item.index;
                ActionIcon::new("fa fa-lg fa-trash-o")
                    .tabindex(0)
                    .on_activate(link.callback(move |_| Msg::RemoveUrl(index)))
                    .into()
            })
            .into(),
    ])
}
