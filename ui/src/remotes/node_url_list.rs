use std::rc::Rc;

use anyhow::{bail, Error};
use serde_json::Value;

use yew::virtual_dom::Key;

use pwt::css::FlexFit;
use pwt::state::Store;
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::ManagedField;
use pwt::widget::form::ManagedFieldScopeExt;
use pwt::widget::form::{Field, ManagedFieldContext, ManagedFieldMaster, ManagedFieldState};
use pwt::widget::{ActionIcon, Button, Column, Container, Fa, Row, Trigger};
use pwt::{css, prelude::*};

use proxmox_yew_comp::{SchemaValidation, Status};

use pdm_api_types::remotes::NodeUrl;
use proxmox_schema::property_string::PropertyString;

use proxmox_schema::api_types::CERT_FINGERPRINT_SHA256_SCHEMA;

use pwt_macros::{builder, widget};

//pub type PdmNodeUrlList = ManagedFieldMaster<PdmNodeUrlField>;

#[widget(comp=ManagedFieldMaster<PdmNodeUrlField>, @input)]
#[derive(Clone, PartialEq, Properties)]
#[builder]
/// A grid field to hold a list of hostname,fingerprint pairs for e.g. [Remote]
///
/// Note: std_props are applied on the grid only, not the toolbar.
pub struct NodeUrlList {
    /// Default value.
    #[builder]
    #[prop_or_default]
    pub default: Vec<PropertyString<NodeUrl>>,
}

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
    state: ManagedFieldState,
    store: Store<Entry>,
    columns: Rc<Vec<DataTableHeader<Entry>>>,
}

pwt::impl_deref_mut_property!(PdmNodeUrlField, state, ManagedFieldState);

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
    type ValidateClosure = bool;

    fn validation_args(props: &Self::Properties) -> Self::ValidateClosure {
        props.input_props.required
    }

    fn validator(required: &Self::ValidateClosure, value: &Value) -> Result<Value, Error> {
        let data = serde_json::from_value::<Vec<PropertyString<NodeUrl>>>(value.clone())?;
        if data.is_empty() && *required {
            bail!("at least one entry required!")
        }
        Ok(value.clone())
    }

    fn create(ctx: &pwt::widget::form::ManagedFieldContext<Self>) -> Self {
        let store = Store::with_extract_key(|entry: &Entry| Key::from(entry.index))
            .on_change(ctx.link().callback(|_| Msg::DataChange));
        let columns = columns(ctx);
        let value = Value::Null;
        let default = ctx
            .props()
            .default
            .iter()
            .filter_map(|n| serde_json::to_value(n).ok())
            .collect();
        let state = ManagedFieldState::new(value, default);
        let mut this = Self {
            state,
            store,
            columns,
        };
        this.set_nodes(
            ctx.props()
                .default
                .clone()
                .into_iter()
                .map(|n| n.into_inner())
                .collect(),
        );
        this
    }

    fn changed(&mut self, ctx: &ManagedFieldContext<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();
        if old_props.default != props.default {
            let default: Value = props
                .default
                .iter()
                .filter_map(|n| serde_json::to_value(n).ok())
                .collect();
            ctx.link().update_default(default.clone());
            self.sync_from_value(default);
        }
        true
    }

    fn value_changed(&mut self, _ctx: &ManagedFieldContext<Self>) {
        match self.state.value {
            Value::Null => self.sync_from_value(self.state.default.clone()),
            _ => self.sync_from_value(self.state.value.clone()),
        }
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

    fn view(&self, ctx: &pwt::widget::form::ManagedFieldContext<Self>) -> Html {
        let table = DataTable::new(self.columns.clone(), self.store.clone())
            .border(true)
            .class(css::FlexFit);

        let toolbar = Row::new()
            .with_child(
                Button::new(tr!("Add"))
                    .class(css::ColorScheme::Primary)
                    .icon_class("fa fa-plus-circle")
                    .on_activate({
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
            )
            .with_flex_spacer()
            .with_optional_child(self.state.result.clone().err().map(|err| {
                Row::new()
                    .class(css::AlignItems::Center)
                    .gap(2)
                    .with_child(Fa::from(Status::Warning).class(css::FontColor::Error))
                    .with_child(err)
            }));

        Column::new()
            .class(FlexFit)
            .gap(2)
            .with_child(
                Container::from_widget_props(ctx.props().std_props.clone(), None)
                    .class(FlexFit)
                    .with_child(table),
            )
            .with_child(toolbar)
            .into()
    }
}

fn columns(ctx: &ManagedFieldContext<PdmNodeUrlField>) -> Rc<Vec<DataTableHeader<Entry>>> {
    let link = ctx.link().clone();

    Rc::new(vec![
        DataTableColumn::new(tr!("Hostname/IP"))
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
                        .placeholder(tr!("Use trusted certificate"))
                        .on_change(link.callback(move |value| Msg::UpdateFingerprint(index, value)))
                        .with_trigger(
                            Trigger::new(if !fingerprint.is_empty() {
                                "fa fa-times"
                            } else {
                                Default::default()
                            })
                            .tip(tr!("Clear"))
                            .on_activate(
                                link.callback(move |_| {
                                    Msg::UpdateFingerprint(index, "".to_string())
                                }),
                            ),
                            true,
                        )
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
