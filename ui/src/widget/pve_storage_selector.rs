use std::rc::Rc;

use anyhow::{format_err, Error};
use proxmox_human_byte::HumanByte;
use yew::{
    html,
    html::{IntoEventCallback, IntoPropValue},
    virtual_dom::Key,
    AttrValue, Callback, Component, Properties,
};

use pwt::{
    css::FlexFit,
    props::{FieldBuilder, LoadCallback, WidgetBuilder, WidgetStyleBuilder},
    state::Store,
    tr,
    widget::{
        data_table::{DataTable, DataTableColumn, DataTableHeader},
        form::{Selector, SelectorRenderArgs, ValidateFn},
        GridPicker,
    },
};
use pwt_macros::{builder, widget};

use pdm_client::types::{StorageContent, StorageInfo};

#[widget(comp=PveStorageSelectorComp, @input)]
#[derive(Clone, Properties, PartialEq)]
#[builder]
pub struct PveStorageSelector {
    /// The default value
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub default: Option<AttrValue>,

    /// Change callback
    #[builder_cb(IntoEventCallback, into_event_callback, Option<AttrValue>)]
    #[prop_or_default]
    pub on_change: Option<Callback<Option<AttrValue>>>,

    /// The remote to select the network from
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub remote: AttrValue,

    /// The node to query
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub node: Option<AttrValue>,

    /// The target node for the storage
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub target: Option<AttrValue>,

    /// The content types to show
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub content_types: Option<Vec<StorageContent>>,

    /// If set, automatically selects the first value from the store (if no default is selected)
    #[builder]
    #[prop_or(true)]
    pub autoselect: bool,
}

impl PveStorageSelector {
    pub fn new(remote: impl IntoPropValue<AttrValue>) -> Self {
        yew::props!(Self {
            remote: remote.into_prop_value()
        })
    }
}

pub struct PveStorageSelectorComp {
    store: Store<StorageInfo>,
    load_callback: LoadCallback<Vec<StorageInfo>>,
    validate_fn: pwt::widget::form::ValidateFn<(String, Store<StorageInfo>)>,
}

impl PveStorageSelectorComp {
    async fn get_storage_list(
        remote: AttrValue,
        node: Option<AttrValue>,
        content: Option<Vec<StorageContent>>,
        target: Option<AttrValue>,
    ) -> Result<Vec<StorageInfo>, Error> {
        let mut storages = crate::pdm_client()
            .pve_list_storages(
                &remote,
                &node.unwrap_or(AttrValue::from("localhost")),
                content,
                Some(true),
                Some(true),
                None,
                target.as_ref().map(AttrValue::to_string),
            )
            .await?;

        storages.sort_by(|a, b| a.storage.cmp(&b.storage));
        Ok(storages)
    }

    fn create_load_callback(ctx: &yew::Context<Self>) -> LoadCallback<Vec<StorageInfo>> {
        let props = ctx.props();
        let remote = props.remote.clone();
        let node = props.node.clone();
        let target = props.target.clone();
        let content_types = props.content_types.clone();

        (move || {
            Self::get_storage_list(
                remote.clone(),
                node.clone(),
                content_types.clone(),
                target.clone(),
            )
        })
        .into()
    }
}

impl Component for PveStorageSelectorComp {
    type Message = ();
    type Properties = PveStorageSelector;

    fn create(ctx: &yew::Context<Self>) -> Self {
        let validate_fn = ValidateFn::new(|(value, store): &(String, Store<StorageInfo>)| {
            store
                .read()
                .iter()
                .find(|item| item.storage == *value)
                .ok_or_else(|| format_err!("no such item"))
                .map(|_| ())
        });
        Self {
            store: Store::with_extract_key(|storage: &StorageInfo| {
                Key::from(storage.storage.as_str())
            }),
            load_callback: Self::create_load_callback(ctx),
            validate_fn,
        }
    }

    fn changed(&mut self, ctx: &yew::Context<Self>, old: &Self::Properties) -> bool {
        let props = ctx.props();

        if old.target != props.target
            || old.remote != props.remote
            || old.node != props.node
            || old.content_types != props.content_types
        {
            self.load_callback = Self::create_load_callback(ctx);
        }

        true
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();

        let on_change = {
            let on_change = props.on_change.clone();
            let store = self.store.clone();
            move |key: Key| {
                if let Some(on_change) = &on_change {
                    let result = store
                        .read()
                        .iter()
                        .find(|e| key == store.extract_key(e))
                        .map(|e| e.storage.clone().into());
                    on_change.emit(result);
                }
            }
        };
        Selector::new(
            self.store.clone(),
            move |args: &SelectorRenderArgs<Store<StorageInfo>>| {
                GridPicker::new(
                    DataTable::new(columns(), args.store.clone())
                        .min_width(300)
                        .header_focusable(false)
                        .class(FlexFit),
                )
                .selection(args.selection.clone())
                .on_select(args.controller.on_select_callback())
                .into()
            },
        )
        .loader(self.load_callback.clone())
        .with_std_props(&props.std_props)
        .with_input_props(&props.input_props)
        .autoselect(props.autoselect)
        .validate(self.validate_fn.clone())
        .on_change(on_change)
        .default(props.default.clone())
        .into()
    }
}

fn columns() -> Rc<Vec<DataTableHeader<StorageInfo>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Name"))
            .get_property(|entry: &StorageInfo| &entry.storage)
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Type"))
            .get_property(|entry: &StorageInfo| &entry.ty)
            .into(),
        DataTableColumn::new(tr!("Avail"))
            .get_property_owned(|entry: &StorageInfo| entry.used.unwrap_or_default())
            .render(|entry: &StorageInfo| match entry.used {
                Some(used) => html! {format!("{:.2}", HumanByte::new_decimal(used as f64))},
                None => html! {"-"},
            })
            .into(),
        DataTableColumn::new(tr!("Capacity"))
            .get_property_owned(|entry: &StorageInfo| entry.avail.unwrap_or_default())
            .render(|entry: &StorageInfo| match entry.avail {
                Some(used) => html! { format!("{:.2}", HumanByte::new_decimal(used as f64))},
                None => html! {"-"},
            })
            .into(),
    ])
}
