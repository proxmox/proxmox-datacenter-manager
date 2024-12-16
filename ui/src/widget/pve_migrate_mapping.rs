use std::{collections::HashSet, rc::Rc};

use anyhow::{bail, Error};
use serde_json::Value;
use yew::{html::IntoPropValue, virtual_dom::Key, AttrValue, Properties};

use proxmox_schema::{ObjectSchema, Schema};
use pwt::{
    props::{ContainerBuilder, CssBorderBuilder, ExtractPrimaryKey, FieldBuilder, WidgetBuilder},
    state::Store,
    tr,
    widget::{
        data_table::{DataTable, DataTableColumn, DataTableHeader},
        error_message,
        form::{ManagedField, ManagedFieldContext, ManagedFieldMaster, ManagedFieldState},
        Column,
    },
    AsyncPool,
};
use pwt_macros::{builder, widget};

use pdm_client::types::StorageContent;

use crate::pve::{
    utils::{foreach_drive_lxc, foreach_drive_qemu},
    GuestInfo,
};

use super::{PveNetworkSelector, PveStorageSelector};

#[derive(PartialEq, Eq, Clone, PartialOrd, Ord)]
pub enum MapType {
    Storage,
    Network,
}

impl std::fmt::Display for MapType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            &(match self {
                MapType::Storage => tr!("Storage"),
                MapType::Network => tr!("Network"),
            }),
        )
    }
}

#[derive(PartialEq, Clone)]
pub struct MapEntry {
    map_type: MapType,
    source: AttrValue,
    target: AttrValue,
}

impl ExtractPrimaryKey for MapEntry {
    fn extract_key(&self) -> Key {
        Key::from(format!("{}-{}", self.map_type, self.source))
    }
}

#[widget(comp=ManagedFieldMaster<PveMigrateMapComp>, @input)]
#[derive(Clone, Properties, PartialEq)]
#[builder]
pub struct PveMigrateMap {
    /// Force value.
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub value: Option<Vec<String>>,

    /// The default value
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub default: Option<Vec<String>>,

    /// The remote to select the network from
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub remote: AttrValue,

    pub guest_info: GuestInfo,

    /// The node to query
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or(AttrValue::from("localhost"))]
    pub node: AttrValue,

    /// The target node for the storage
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub target: Option<AttrValue>,

    /// The content types to show
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub content_types: Option<Vec<StorageContent>>,
}

impl PveMigrateMap {
    pub fn new(remote: impl IntoPropValue<AttrValue>, guest_info: GuestInfo) -> Self {
        yew::props!(Self {
            remote: remote.into_prop_value(),
            guest_info,
        })
    }
}

pub enum Msg {
    LoadResult(Result<(Vec<String>, Vec<String>), Error>),
    DataChanged(MapEntry),
}

pub struct PveMigrateMapComp {
    store: Store<MapEntry>,
    last_err: Option<String>,
    _async_pool: AsyncPool,
}

// HACK!, our rust schema does not support `keyAlias`  yet, so we parse it into a generic value`
static NET_WORKAROUND_SCHEMA: Schema = ObjectSchema::new("", &[])
    .additional_properties(true)
    .schema();

impl PveMigrateMapComp {
    async fn load_storages(
        remote: AttrValue,
        guest_info: GuestInfo,
    ) -> Result<(Vec<String>, Vec<String>), Error> {
        let (storages, networks) = match guest_info.guest_type {
            crate::pve::GuestType::Qemu => {
                let config = crate::pdm_client()
                    .pve_qemu_config(
                        &remote,
                        None,
                        guest_info.vmid,
                        pdm_api_types::ConfigurationState::Active,
                        None,
                    )
                    .await?;

                let mut storages = HashSet::new();

                foreach_drive_qemu(&config, |key, value| {
                    let file = value.get_file();
                    if let Some(captures) = pdm_client::types::VOLUME_ID.captures(file) {
                        let storage = captures.get(1).unwrap();
                        storages.insert(storage.as_str().to_string());
                    } else {
                        log::error!("could not parse 'file' property of '{key}");
                    }
                })?;

                let mut networks = HashSet::new();

                let nets = serde_json::to_value(&config.net)?;
                for (_key, net) in nets.as_object().unwrap() {
                    let net = NET_WORKAROUND_SCHEMA.parse_property_string(net.as_str().unwrap())?;

                    if let Some(bridge) = net.get("bridge") {
                        networks.insert(bridge.as_str().unwrap().to_string());
                    }
                }

                (
                    storages.into_iter().collect(),
                    networks.into_iter().collect(),
                )
            }
            crate::pve::GuestType::Lxc => {
                let config = crate::pdm_client()
                    .pve_lxc_config(
                        &remote,
                        None,
                        guest_info.vmid,
                        pdm_api_types::ConfigurationState::Active,
                        None,
                    )
                    .await?;

                let mut storages = HashSet::new();

                foreach_drive_lxc(&config, |key, value| {
                    let volume = value.get_volume();
                    if let Some(captures) = pdm_client::types::VOLUME_ID.captures(volume) {
                        let storage = captures.get(1).unwrap();
                        storages.insert(storage.as_str().to_string());
                    } else {
                        log::error!("could not parse 'file' property of '{key}");
                    }
                })?;

                let mut networks = HashSet::new();

                let nets = serde_json::to_value(&config.net)?;
                for (_key, net) in nets.as_object().unwrap() {
                    let net = NET_WORKAROUND_SCHEMA.parse_property_string(net.as_str().unwrap())?;

                    if let Some(bridge) = net.get("bridge") {
                        networks.insert(bridge.as_str().unwrap().to_string());
                    }
                }

                (
                    storages.into_iter().collect(),
                    networks.into_iter().collect(),
                )
            }
        };

        Ok((storages, networks))
    }
}

impl ManagedField for PveMigrateMapComp {
    type Properties = PveMigrateMap;
    type Message = Msg;
    type ValidateClosure = bool;

    fn validation_args(props: &Self::Properties) -> Self::ValidateClosure {
        props.input_props.required
    }

    fn validator(
        required: &Self::ValidateClosure,
        value: &serde_json::Value,
    ) -> Result<serde_json::Value, Error> {
        if value == &serde_json::Value::Null && *required {
            bail!("invalid value")
        }
        Ok(value.clone())
    }

    fn setup(props: &Self::Properties) -> ManagedFieldState {
        ManagedFieldState {
            value: props.value.clone().into(),
            valid: Ok(()),
            default: props.default.clone().into(),
            radio_group: false,
            unique: false,
        }
    }

    fn update(&mut self, ctx: &ManagedFieldContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadResult(res) => match res {
                Ok((storages, networks)) => {
                    self.last_err = None;
                    let mut data = Vec::new();
                    for source in storages {
                        data.push(MapEntry {
                            map_type: MapType::Storage,
                            source: source.into(),
                            target: AttrValue::from(""),
                        });
                    }
                    for source in networks {
                        data.push(MapEntry {
                            map_type: MapType::Network,
                            source: source.into(),
                            target: AttrValue::from(""),
                        });
                    }
                    self.store.set_data(data);
                }
                Err(err) => self.last_err = Some(err.to_string()),
            },
            Msg::DataChanged(entry) => {
                {
                    // scope the write guard
                    let mut store = self.store.write();
                    let key = entry.extract_key();
                    if let Some(store_entry) = store.iter_mut().find(|e| e.extract_key() == key) {
                        (*store_entry).target = entry.target.clone();
                    }
                }
                let list: Vec<_> = self
                    .store
                    .read()
                    .iter()
                    .filter_map(|e| {
                        let ty = match e.map_type {
                            MapType::Storage => "s",
                            MapType::Network => "n",
                        };
                        if e.target.is_empty() {
                            None
                        } else {
                            Some(Value::String(format!("{ty}:{}={}", e.source, e.target)))
                        }
                    })
                    .collect();

                if list.len() != self.store.data_len() {
                    ctx.link().update_value(Value::Null);
                } else {
                    ctx.link().update_value(Value::Array(list));
                }
            }
        }
        true
    }

    fn create(ctx: &ManagedFieldContext<'_, Self>) -> Self {
        let props = ctx.props();
        let remote = props.remote.clone();
        let guest_info = props.guest_info;
        let link = ctx.link();

        let _async_pool = AsyncPool::new();
        _async_pool.spawn(async move {
            link.send_message(Msg::LoadResult(
                Self::load_storages(remote, guest_info).await,
            ));
        });

        Self {
            store: Store::new(),
            _async_pool,
            last_err: None,
        }
    }

    fn view(&self, ctx: &ManagedFieldContext<'_, Self>) -> yew::Html {
        let props = ctx.props();
        let err = self.last_err.as_ref().map(|err| error_message(err));
        Column::new()
            .with_std_props(&props.std_props)
            .with_child(
                DataTable::new(columns(ctx, props.remote.clone()), self.store.clone()).border(true),
            )
            .with_optional_child(err)
            .into()
    }
}

fn columns(
    ctx: &ManagedFieldContext<'_, PveMigrateMapComp>,
    remote: AttrValue,
) -> Rc<Vec<DataTableHeader<MapEntry>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Type"))
            .get_property(|entry: &MapEntry| &entry.map_type)
            .flex(1)
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Source"))
            .get_property(|entry: &MapEntry| &entry.source)
            .flex(2)
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Target"))
            .flex(2)
            .render({
                let link = ctx.link();
                move |entry: &MapEntry| match entry.map_type {
                    MapType::Storage => PveStorageSelector::new(remote.clone())
                        .default(entry.target.clone())
                        .on_change({
                            let link = link.clone();
                            let entry = entry.clone();
                            move |value: Option<AttrValue>| {
                                let mut entry = entry.clone();
                                entry.target = value.unwrap_or("".into());
                                link.send_message(Msg::DataChanged(entry));
                            }
                        })
                        .required(true)
                        .into(),
                    MapType::Network => PveNetworkSelector::new(remote.clone())
                        .default(entry.target.clone())
                        .on_change({
                            let link = link.clone();
                            let entry = entry.clone();
                            move |value: Option<AttrValue>| {
                                let mut entry = entry.clone();
                                entry.target = value.unwrap_or("".into());
                                link.send_message(Msg::DataChanged(entry));
                            }
                        })
                        .required(true)
                        .into(),
                }
            })
            .into(),
    ])
}
