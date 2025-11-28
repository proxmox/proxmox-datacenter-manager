use std::cmp::Ordering;
use std::collections::HashSet;
use std::rc::Rc;
use std::str::FromStr;

use anyhow::{anyhow, Error};
use pwt::widget::{error_message, Column};
use yew::virtual_dom::{Key, VNode};
use yew::{Callback, Component, Context, Html, Properties};

use pdm_client::types::{ListController, ListVnet, ListZone, SdnObjectState};
use pwt::css;
use pwt::props::{ContainerBuilder, ExtractPrimaryKey, WidgetBuilder};
use pwt::state::{Selection, SlabTree, TreeStore};
use pwt::tr;
use pwt::widget::data_table::{
    DataTable, DataTableColumn, DataTableHeader, DataTableRowRenderArgs,
};
use pwt::widget::{Fa, Row};
use pwt_macros::widget;

use crate::sdn::evpn::evpn_panel::DetailPanel;
use crate::sdn::evpn::EvpnRouteTarget;

#[widget(comp=RemoteTreeComponent)]
#[derive(Clone, PartialEq, Properties)]
pub struct RemoteTree {
    zones: Rc<Vec<ListZone>>,
    vnets: Rc<Vec<ListVnet>>,
    controllers: Rc<Vec<ListController>>,
    on_select: Callback<Option<DetailPanel>>,
}

impl RemoteTree {
    pub fn new(
        zones: Rc<Vec<ListZone>>,
        vnets: Rc<Vec<ListVnet>>,
        controllers: Rc<Vec<ListController>>,
        on_select: Callback<Option<DetailPanel>>,
    ) -> Self {
        yew::props!(Self {
            zones,
            vnets,
            controllers,
            on_select,
        })
    }
}

#[derive(Clone, PartialEq, Debug)]
struct RemoteData {
    id: String,
    asn: u32,
}

#[derive(Clone, PartialEq, Debug)]
struct ZoneData {
    id: String,
    remote: String,
    route_target: EvpnRouteTarget,
    import_targets: HashSet<EvpnRouteTarget>,
    state: Option<SdnObjectState>,
    controller_id: String,
}

#[derive(Clone, PartialEq, Debug)]
struct VnetData {
    parent_remote: String,
    parent_zone: String,
    id: String,
    zone: String,
    remote: String,
    route_target: EvpnRouteTarget,
    imported: bool,
    external: bool,
    state: Option<SdnObjectState>,
}

#[derive(Clone, PartialEq, Debug)]
enum RemoteTreeEntry {
    Root,
    Remote(RemoteData),
    Zone(ZoneData),
    Vnet(VnetData),
}

impl ExtractPrimaryKey for RemoteTreeEntry {
    fn extract_key(&self) -> Key {
        match self {
            Self::Root => Key::from("root"),
            Self::Remote(remote) => Key::from(remote.id.clone()),
            Self::Zone(zone) => Key::from(format!("{}/{}", zone.remote, zone.id)),
            Self::Vnet(vnet) => Key::from(format!(
                "{}/{}/{}/{}/{}",
                vnet.remote, vnet.parent_remote, vnet.parent_zone, vnet.zone, vnet.id
            )),
        }
    }
}

impl RemoteTreeEntry {
    fn name(&self) -> Option<String> {
        match self {
            RemoteTreeEntry::Root => None,
            RemoteTreeEntry::Remote(remote) => {
                Some(format!("{} (ASN: {})", &remote.id, remote.asn))
            }
            RemoteTreeEntry::Zone(zone) => Some(zone.id.to_string()),
            RemoteTreeEntry::Vnet(vnet) => Some(vnet.id.to_string()),
        }
    }

    fn remote(&self) -> Option<&str> {
        match self {
            RemoteTreeEntry::Root => None,
            RemoteTreeEntry::Remote(remote) => Some(&remote.id),
            RemoteTreeEntry::Zone(zone) => Some(&zone.remote),
            RemoteTreeEntry::Vnet(vnet) => Some(&vnet.remote),
        }
    }

    fn l2vni(&self) -> Option<u32> {
        match self {
            RemoteTreeEntry::Vnet(vnet) => Some(vnet.route_target.vni),
            _ => None,
        }
    }

    fn l3vni(&self) -> Option<u32> {
        match self {
            RemoteTreeEntry::Zone(zone) => Some(zone.route_target.vni),
            _ => None,
        }
    }

    fn external(&self) -> Option<bool> {
        match self {
            RemoteTreeEntry::Vnet(vnet) => Some(vnet.external),
            _ => None,
        }
    }

    fn imported(&self) -> Option<bool> {
        match self {
            RemoteTreeEntry::Vnet(vnet) => Some(vnet.imported),
            _ => None,
        }
    }
}

fn zones_to_remote_view(
    controllers: &[ListController],
    zones: &[ListZone],
    vnets: &[ListVnet],
) -> Result<SlabTree<RemoteTreeEntry>, Error> {
    let mut tree = SlabTree::new();

    let mut root = tree.set_root(RemoteTreeEntry::Root);
    root.set_expanded(true);

    for zone in zones {
        let zone_data = &zone.zone;

        let zone_controller_id = zone_data.controller.as_ref().ok_or_else(|| {
            anyhow!(tr!(
                "EVPN zone {} has no controller defined!",
                zone_data.zone
            ))
        })?;

        let controller = controllers
            .iter()
            .find(|controller| {
                controller.remote == zone.remote
                    && zone_controller_id == &controller.controller.controller
            })
            .ok_or_else(|| {
                anyhow!(tr!(
                    "Could not find Controller for EVPN zone {}",
                    zone_data.zone
                ))
            })?;

        let route_target = EvpnRouteTarget {
            asn: controller.controller.asn.ok_or_else(|| {
                anyhow!(tr!(
                    "EVPN controller {} has no ASN defined!",
                    controller.controller.controller
                ))
            })?,
            vni: zone.zone.vrf_vxlan.ok_or_else(|| {
                anyhow!(tr!("EVPN Zone {} has no VXLAN ID defined!", zone_data.zone))
            })?,
        };

        let import_targets = zone_data
            .rt_import
            .iter()
            .flat_map(|rt_import| rt_import.split(',').map(EvpnRouteTarget::from_str))
            .collect::<Result<_, Error>>()?;

        let remote_entry = root.children_mut().find(|remote_entry| {
            if let RemoteTreeEntry::Remote(remote) = remote_entry.record() {
                return remote.id == zone.remote;
            }

            false
        });

        let zone_entry = RemoteTreeEntry::Zone(ZoneData {
            id: zone_data.zone.clone(),
            remote: zone.remote.clone(),
            route_target,
            import_targets,
            state: zone_data.state,
            controller_id: controller.controller.controller.clone(),
        });

        if let Some(mut remote_entry) = remote_entry {
            remote_entry.append(zone_entry);
        } else {
            let mut new_remote_entry = root.append(RemoteTreeEntry::Remote(RemoteData {
                id: zone.remote.clone(),
                asn: route_target.asn,
            }));

            new_remote_entry.set_expanded(true);
            new_remote_entry.append(zone_entry);
        };
    }

    for vnet in vnets {
        let vnet_data = &vnet.vnet;

        let vnet_zone_id = vnet_data
            .zone
            .as_ref()
            .ok_or_else(|| anyhow!(tr!("VNet {} has no zone defined!", vnet_data.vnet)))?;

        let Some(zone) = zones
            .iter()
            .find(|zone| zone.remote == vnet.remote && vnet_zone_id == &zone.zone.zone)
        else {
            // this VNet is not part of an EVPN zone, skip it
            continue;
        };

        let zone_controller_id = zone.zone.controller.as_ref().ok_or_else(|| {
            anyhow!(tr!(
                "EVPN zone {} has no controller defined!",
                &zone.zone.zone
            ))
        })?;

        let controller = controllers
            .iter()
            .find(|controller| {
                controller.remote == zone.remote
                    && zone_controller_id == &controller.controller.controller
            })
            .ok_or_else(|| {
                anyhow!(tr!(
                    "Controller of EVPN zone {} does not exist",
                    zone.zone.zone
                ))
            })?;

        let controller_asn = controller.controller.asn.ok_or_else(|| {
            anyhow!(tr!(
                "EVPN controller {} has no ASN defined!",
                controller.controller.controller
            ))
        })?;

        let zone_target = EvpnRouteTarget {
            asn: controller_asn,
            vni: zone
                .zone
                .vrf_vxlan
                .ok_or_else(|| anyhow!(tr!("EVPN Zone {} has no VRF VNI", zone.zone.zone)))?,
        };

        let vnet_target = EvpnRouteTarget {
            asn: controller_asn,
            vni: vnet_data
                .tag
                .ok_or_else(|| anyhow!(tr!("VNet {} has no VNI", vnet_data.vnet)))?,
        };

        for mut remote_entry in root.children_mut() {
            for mut zone_entry in remote_entry.children_mut() {
                if let RemoteTreeEntry::Zone(zone) = zone_entry.record() {
                    let imported = if zone.route_target == zone_target {
                        false
                    } else if zone.import_targets.contains(&zone_target)
                        || zone.import_targets.contains(&vnet_target)
                    {
                        true
                    } else {
                        continue;
                    };

                    zone_entry.append(RemoteTreeEntry::Vnet(VnetData {
                        id: vnet.vnet.vnet.clone(),
                        remote: vnet.remote.clone(),
                        zone: vnet.vnet.zone.clone().unwrap(),
                        route_target: vnet_target,
                        imported,
                        external: zone.remote != vnet.remote,
                        parent_remote: zone.remote.clone(),
                        parent_zone: zone.id.clone(),
                        state: vnet.vnet.state,
                    }));
                }
            }
        }
    }

    Ok(tree)
}
pub struct RemoteTreeComponent {
    store: TreeStore<RemoteTreeEntry>,
    selection: Selection,
    error_msg: Option<String>,
    columns: Rc<Vec<DataTableHeader<RemoteTreeEntry>>>,
}

fn name_remote_sorter(a: &RemoteTreeEntry, b: &RemoteTreeEntry) -> Ordering {
    (a.name(), a.remote()).cmp(&(b.name(), b.remote()))
}

fn default_sorter(a: &RemoteTreeEntry, b: &RemoteTreeEntry) -> Ordering {
    (
        a.external(),
        a.imported(),
        a.remote(),
        a.name(),
        a.l3vni(),
        a.l2vni(),
    )
        .cmp(&(
            b.external(),
            b.imported(),
            b.remote(),
            b.name(),
            b.l3vni(),
            b.l2vni(),
        ))
}

impl RemoteTreeComponent {
    fn columns(store: TreeStore<RemoteTreeEntry>) -> Rc<Vec<DataTableHeader<RemoteTreeEntry>>> {
        Rc::new(vec![
            DataTableColumn::new(tr!("Name"))
                .tree_column(store)
                .sorter(name_remote_sorter)
                .render(|item: &RemoteTreeEntry| {
                    let name = item.name();

                    name.map(|name| {
                        let mut row = Row::new().class(css::AlignItems::Baseline).gap(2);

                        row = match item {
                            RemoteTreeEntry::Remote(_) => row.with_child(Fa::new("server")),
                            RemoteTreeEntry::Zone(_) => row.with_child(Fa::new("th")),
                            _ => row,
                        };

                        row = row.with_child(name);

                        Html::from(row)
                    })
                    .unwrap_or_default()
                })
                .flex(2)
                .into(),
            DataTableColumn::new(tr!("Remote"))
                .get_property(|item: &RemoteTreeEntry| match item {
                    RemoteTreeEntry::Zone(zone) => zone.remote.as_str(),
                    RemoteTreeEntry::Vnet(vnet) => vnet.remote.as_str(),
                    _ => "",
                })
                .flex(1)
                .into(),
            DataTableColumn::new(tr!("L3VNI"))
                .render(|item: &RemoteTreeEntry| item.l3vni().map(VNode::from).unwrap_or_default())
                .sorter(|a: &RemoteTreeEntry, b: &RemoteTreeEntry| a.l3vni().cmp(&b.l3vni()))
                .flex(1)
                .into(),
            DataTableColumn::new(tr!("L2VNI"))
                .render(|item: &RemoteTreeEntry| item.l2vni().map(VNode::from).unwrap_or_default())
                .sorter(|a: &RemoteTreeEntry, b: &RemoteTreeEntry| a.l2vni().cmp(&b.l2vni()))
                .flex(1)
                .into(),
            DataTableColumn::new(tr!("External"))
                .get_property_owned(|item: &RemoteTreeEntry| match item {
                    RemoteTreeEntry::Vnet(vnet) if vnet.external => tr!("Yes"),
                    RemoteTreeEntry::Vnet(vnet) if !vnet.external => tr!("No"),
                    _ => String::new(),
                })
                .flex(1)
                .into(),
            DataTableColumn::new(tr!("Imported"))
                .get_property_owned(|item: &RemoteTreeEntry| match item {
                    RemoteTreeEntry::Vnet(vnet) if vnet.imported => tr!("Yes"),
                    RemoteTreeEntry::Vnet(vnet) if !vnet.imported => tr!("No"),
                    _ => String::new(),
                })
                .flex(1)
                .into(),
        ])
    }
}

impl Component for RemoteTreeComponent {
    type Properties = RemoteTree;
    type Message = ();

    fn create(ctx: &Context<Self>) -> Self {
        let store = TreeStore::new().view_root(false);
        let columns = Self::columns(store.clone());

        let on_select = ctx.props().on_select.clone();
        let selection_store = store.clone();
        let selection = Selection::new().on_select(move |selection: Selection| {
            if let Some(selected_key) = selection.selected_key() {
                let read_guard = selection_store.read();

                if let Some(node) = read_guard.lookup_node(&selected_key) {
                    match node.record() {
                        RemoteTreeEntry::Zone(zone) => {
                            on_select.emit(Some(DetailPanel::Zone {
                                remote: zone.remote.clone(),
                                zone: zone.id.clone(),
                            }));
                        }
                        RemoteTreeEntry::Vnet(vnet) => {
                            on_select.emit(Some(DetailPanel::Vnet {
                                remote: vnet.remote.clone(),
                                vnet: vnet.id.clone(),
                            }));
                        }
                        _ => on_select.emit(None),
                    }
                }
            } else {
                on_select.emit(None);
            }
        });

        let mut error_msg = None;

        match zones_to_remote_view(
            &ctx.props().controllers,
            &ctx.props().zones,
            &ctx.props().vnets,
        ) {
            Ok(data) => {
                store.set_data(data);
                store.set_sorter(default_sorter);
            }
            Err(error) => {
                error_msg = Some(error.to_string());
            }
        }

        Self {
            store,
            selection,
            columns,
            error_msg,
        }
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        let mut table_column = Column::new().class(pwt::css::FlexFit).with_child(
            DataTable::new(self.columns.clone(), self.store.clone())
                .striped(false)
                .selection(self.selection.clone())
                .row_render_callback(|args: &mut DataTableRowRenderArgs<RemoteTreeEntry>| {
                    match args.record() {
                        RemoteTreeEntry::Vnet(vnet) if vnet.external || vnet.imported => {
                            args.add_class("pwt-opacity-50");
                        }
                        RemoteTreeEntry::Remote(_) => args.add_class("pwt-bg-color-surface"),
                        _ => (),
                    };
                })
                .class(css::FlexFit),
        );

        if let Some(msg) = &self.error_msg {
            table_column.add_child(error_message(msg.as_ref()));
        }

        table_column.into()
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        if !Rc::ptr_eq(&ctx.props().zones, &old_props.zones)
            || !Rc::ptr_eq(&ctx.props().vnets, &old_props.vnets)
            || !Rc::ptr_eq(&ctx.props().controllers, &old_props.controllers)
        {
            match zones_to_remote_view(
                &ctx.props().controllers,
                &ctx.props().zones,
                &ctx.props().vnets,
            ) {
                Ok(data) => {
                    self.store.write().update_root_tree(data);
                    self.store.set_sorter(default_sorter);

                    self.error_msg = None;
                }
                Err(error) => {
                    self.error_msg = Some(error.to_string());
                }
            }

            return true;
        }

        false
    }
}
