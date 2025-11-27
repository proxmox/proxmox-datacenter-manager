use std::rc::Rc;
use std::str::FromStr;

use anyhow::{bail, Error};
use pdm_api_types::resource::ResourceType;
use pwt::css;
use pwt::widget::{ActionIcon, Button, Column, Row};
use serde_json::Value;
use yew::virtual_dom::Key;

use pwt::prelude::*;
use pwt::state::Store;
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::{
    Combobox, Field, ManagedField, ManagedFieldContext, ManagedFieldMaster, ManagedFieldState,
};
use pwt_macros::widget;

use pdm_api_types::views::{
    EnumMatcher, FilterRule, StringMatcher, FILTER_RULE_LIST_SCHEMA, FILTER_RULE_SCHEMA,
};

use crate::widget::RemoteSelector;

#[derive(PartialEq, Clone)]
struct FilterRuleEntry {
    index: usize,
    filter: Option<FilterRule>,
}

#[derive(PartialEq, Clone, Copy)]
enum FilterRuleType {
    ResourceType,
    ResourcePool,
    ResourceId,
    Tag,
    Remote,
}

impl FromStr for FilterRuleType {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "resource-type" => FilterRuleType::ResourceType,
            "resource-pool" => FilterRuleType::ResourcePool,
            "resource-id" => FilterRuleType::ResourceId,
            "tag" => FilterRuleType::Tag,
            "remote" => FilterRuleType::Remote,
            _ => bail!("unknown filter type"),
        })
    }
}

impl From<FilterRuleType> for AttrValue {
    fn from(value: FilterRuleType) -> Self {
        match value {
            FilterRuleType::ResourceType => "resource-type".into(),
            FilterRuleType::ResourcePool => "resource-pool".into(),
            FilterRuleType::ResourceId => "resource-id".into(),
            FilterRuleType::Tag => "tag".into(),
            FilterRuleType::Remote => "remote".into(),
        }
    }
}

impl From<&FilterRule> for FilterRuleType {
    fn from(value: &FilterRule) -> Self {
        match value {
            FilterRule::ResourceType(_) => FilterRuleType::ResourceType,
            FilterRule::ResourcePool(_) => FilterRuleType::ResourcePool,
            FilterRule::ResourceId(_) => FilterRuleType::ResourceId,
            FilterRule::Tag(_) => FilterRuleType::Tag,
            FilterRule::Remote(_) => FilterRuleType::Remote,
        }
    }
}

#[widget(comp=ManagedFieldMaster<ViewFilterSelectorComp>, @input)]
#[derive(PartialEq, Clone, Properties)]
pub struct ViewFilterSelector {}

impl ViewFilterSelector {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

pub struct ViewFilterSelectorComp {
    store: Store<FilterRuleEntry>,
}

impl ViewFilterSelectorComp {
    fn update_value(&self, ctx: &ManagedFieldContext<Self>) {
        let store = self.store.read();
        let value: Vec<_> = store
            .iter()
            .map(|entry| entry.filter.as_ref().map(|filter| filter.to_string()))
            .collect();

        ctx.link().update_value(value);
    }
}

pub enum Msg {
    Add,
    Remove(usize),                   // index
    ChangeFilter(FilterRule, usize), // index
}

impl ManagedField for ViewFilterSelectorComp {
    type Properties = ViewFilterSelector;
    type Message = Msg;
    type ValidateClosure = bool;

    fn validation_args(props: &Self::Properties) -> Self::ValidateClosure {
        props.input_props.required
    }

    fn update(
        &mut self,
        ctx: &pwt::widget::form::ManagedFieldContext<Self>,
        msg: Self::Message,
    ) -> bool {
        match msg {
            Msg::Add => {
                let mut store = self.store.write();
                let index = store.len();
                store.push(FilterRuleEntry {
                    index,
                    filter: None,
                });
                drop(store);
                self.update_value(ctx);
            }
            Msg::Remove(index) => {
                let data: Vec<FilterRuleEntry> = self
                    .store
                    .read()
                    .iter()
                    .filter(move |&item| item.index != index)
                    .cloned()
                    .enumerate()
                    .map(|(index, mut old)| {
                        old.index = index;
                        old
                    })
                    .collect();
                self.store.set_data(data);
                self.update_value(ctx);
            }
            Msg::ChangeFilter(filter_rule, index) => {
                let mut store = self.store.write();
                if let Some(rec) = store.lookup_record_mut(&Key::from(index)) {
                    rec.filter = Some(filter_rule);
                }
                drop(store);
                self.update_value(ctx);
            }
        }

        true
    }

    fn setup(_props: &Self::Properties) -> pwt::widget::form::ManagedFieldState {
        ManagedFieldState::new(Value::Array(Vec::new()), Value::Array(Vec::new()))
    }

    fn validator(required: &Self::ValidateClosure, value: &Value) -> Result<Value, anyhow::Error> {
        FILTER_RULE_LIST_SCHEMA.verify_json(value)?;

        if value.is_null() && *required {
            bail!("value required");
        }

        Ok(value.clone())
    }

    fn create(_ctx: &pwt::widget::form::ManagedFieldContext<Self>) -> Self {
        let store = Store::with_extract_key(|rule: &FilterRuleEntry| Key::from(rule.index));

        Self { store }
    }

    fn value_changed(&mut self, ctx: &ManagedFieldContext<Self>) {
        if let Ok(data) = serde_json::from_value::<Vec<FilterRule>>(ctx.state().value.clone()) {
            self.store.set_data(
                data.into_iter()
                    .enumerate()
                    .map(|(index, filter)| FilterRuleEntry {
                        index,
                        filter: Some(filter),
                    })
                    .collect(),
            );
        }
    }

    fn view(&self, ctx: &pwt::widget::form::ManagedFieldContext<Self>) -> Html {
        let toolbar = Row::new().with_child(
            Button::new(tr!("Add"))
                .class(css::ColorScheme::Primary)
                .icon_class("fa fa-plus-circle")
                .on_activate(ctx.link().callback(|_| Msg::Add)),
        );
        Column::new()
            .gap(2)
            .with_child(
                DataTable::new(columns(ctx), self.store.clone())
                    .border(true)
                    .height(200),
            )
            .with_child(toolbar)
            .into()
    }
}

fn columns(
    ctx: &ManagedFieldContext<ViewFilterSelectorComp>,
) -> Rc<Vec<DataTableHeader<FilterRuleEntry>>> {
    let link = ctx.link().clone();
    let columns = vec![
        DataTableColumn::new(tr!("Type"))
            .render({
                let link = link.clone();
                move |entry: &FilterRuleEntry| {
                    let index = entry.index;
                    let filter_type = entry.filter.as_ref().map(FilterRuleType::from);
                    Combobox::new()
                        .placeholder(tr!("Select"))
                        .required(true)
                        .default(filter_type.map(AttrValue::from))
                        .on_change({
                            let link = link.clone();
                            move |value: String| {
                                let filter = match FilterRuleType::from_str(value.as_str()) {
                                    Ok(FilterRuleType::ResourceType) => {
                                        FilterRule::ResourceType(EnumMatcher(ResourceType::Node))
                                    }
                                    Ok(FilterRuleType::ResourcePool) => FilterRule::ResourcePool(
                                        StringMatcher::Exact(String::new()),
                                    ),
                                    Ok(FilterRuleType::ResourceId) => {
                                        FilterRule::ResourceId(StringMatcher::Exact(String::new()))
                                    }
                                    Ok(FilterRuleType::Tag) => {
                                        FilterRule::Tag(StringMatcher::Exact(String::new()))
                                    }
                                    Ok(FilterRuleType::Remote) => {
                                        FilterRule::Remote(StringMatcher::Exact(String::new()))
                                    }
                                    Err(_) => return,
                                };

                                link.send_message(Msg::ChangeFilter(filter, index));
                            }
                        })
                        .items(Rc::new(vec![
                            FilterRuleType::ResourceType.into(),
                            FilterRuleType::ResourcePool.into(),
                            FilterRuleType::ResourceId.into(),
                            FilterRuleType::Tag.into(),
                            FilterRuleType::Remote.into(),
                        ]))
                        .render_value(|value: &AttrValue| {
                            if value.as_str().is_empty() {
                                return "".into();
                            }
                            match FilterRuleType::from_str(value.as_str()) {
                                Ok(FilterRuleType::ResourceType) => tr!("Resource Type"),
                                Ok(FilterRuleType::ResourcePool) => tr!("Resource Pool"),
                                Ok(FilterRuleType::ResourceId) => tr!("Resource ID"),
                                Ok(FilterRuleType::Tag) => tr!("Tag"),
                                Ok(FilterRuleType::Remote) => tr!("Remote"),
                                Err(err) => tr!("invalid type: {0}", err.to_string()),
                            }
                            .into()
                        })
                        .into()
                }
            })
            .into(),
        DataTableColumn::new(tr!("Value"))
            .render({
                let link = link.clone();
                move |entry: &FilterRuleEntry| {
                    let index = entry.index;

                    let send_change = {
                        let link = link.clone();
                        move |rule: FilterRule| {
                            link.send_message(Msg::ChangeFilter(rule, index));
                        }
                    };
                    match entry.filter.as_ref() {
                        Some(FilterRule::ResourceType(resource_type)) => Combobox::new()
                            .required(true)
                            .value(resource_type.0.to_string())
                            .items(Rc::new(vec![
                                ResourceType::Node.to_string().into(),
                                ResourceType::PveQemu.to_string().into(),
                                ResourceType::PveLxc.to_string().into(),
                                ResourceType::PveStorage.to_string().into(),
                                ResourceType::PveNetwork.to_string().into(),
                                ResourceType::PbsDatastore.to_string().into(),
                            ]))
                            .render_value(|value: &AttrValue| {
                                if value.as_str().is_empty() {
                                    return "".into();
                                }
                                match ResourceType::from_str(value.as_str()) {
                                    Ok(ResourceType::Node) => tr!("Node"),
                                    Ok(ResourceType::PveQemu) => tr!("Virtual Machine"),
                                    Ok(ResourceType::PveLxc) => tr!("Container"),
                                    Ok(ResourceType::PveStorage) => tr!("Storage (Proxmox VE)"),
                                    Ok(ResourceType::PveNetwork) => tr!("Network (Proxmox VE)"),
                                    Ok(ResourceType::PbsDatastore) => tr!("Datastore (Proxmox Backup Server)"),
                                    Err(err) => tr!("invalid type: {0}", err.to_string()),
                                }
                                .into()
                            })
                            .on_change({
                                move |value: String| {
                                    if let Ok(resource_type) =
                                        ResourceType::from_str(value.as_str())
                                    {
                                        send_change(FilterRule::ResourceType(EnumMatcher(
                                            resource_type,
                                        )));
                                    }
                                }
                            })
                            .into(),
                        Some(FilterRule::ResourceId(id)) => Field::new()
                            .value(match id {
                                StringMatcher::Exact(id) => id.clone(),
                            })
                            .required(true)
                            .validate(|value: &String| {
                                let value =
                                    FilterRule::ResourceId(StringMatcher::Exact(value.to_owned()))
                                        .to_string();
                                FILTER_RULE_SCHEMA.parse_simple_value(&value)?;
                                Ok(())
                            })
                            .on_change({
                                move |value: String| {
                                    send_change(FilterRule::ResourceId(StringMatcher::Exact(
                                        value,
                                    )));
                                }
                            })
                            .into(),
                        Some(FilterRule::ResourcePool(pool)) => Field::new()
                            .value(match pool {
                                StringMatcher::Exact(pool) => pool.clone(),
                            })
                            .required(true)
                            .validate(|value: &String| {
                                let value = FilterRule::ResourcePool(StringMatcher::Exact(
                                    value.to_owned(),
                                ))
                                .to_string();
                                FILTER_RULE_SCHEMA.parse_simple_value(&value)?;
                                Ok(())
                            })
                            .on_change({
                                move |value: String| {
                                    send_change(FilterRule::ResourcePool(StringMatcher::Exact(
                                        value,
                                    )));
                                }
                            })
                            .into(),
                        Some(FilterRule::Tag(tag)) => Field::new()
                            .value(match tag {
                                StringMatcher::Exact(tag) => tag.clone(),
                            })
                            .required(true)
                            .validate(|value: &String| {
                                let value = FilterRule::Tag(StringMatcher::Exact(value.to_owned()))
                                    .to_string();
                                FILTER_RULE_SCHEMA.parse_simple_value(&value)?;
                                Ok(())
                            })
                            .on_change({
                                move |value: String| {
                                    send_change(FilterRule::Tag(StringMatcher::Exact(value)));
                                }
                            })
                            .into(),
                        Some(FilterRule::Remote(remote)) => RemoteSelector::new()
                            .value(match remote {
                                StringMatcher::Exact(remote) => remote.clone(),
                            })
                            .required(true)
                            .on_change(move |value| {
                                send_change(FilterRule::Remote(StringMatcher::Exact(value)))
                            })
                            .into(),
                        None => Field::new()
                            .placeholder(tr!("Select Type first"))
                            .disabled(true)
                            .into(),
                    }
                }
            })
            .into(),
        DataTableColumn::new("")
            .width("50px")
            .render(move |entry: &FilterRuleEntry| {
                let index = entry.index;
                ActionIcon::new("fa fa-lg fa-trash-o")
                    .tabindex(0)
                    .on_activate(link.callback(move |_| Msg::Remove(index)))
                    .into()
            })
            .into(),
    ];

    Rc::new(columns)
}
