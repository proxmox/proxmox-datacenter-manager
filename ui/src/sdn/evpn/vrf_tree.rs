use std::cmp::Ordering;
use std::collections::HashSet;
use std::rc::Rc;

use anyhow::{anyhow, Error};
use yew::virtual_dom::{Key, VNode};
use yew::{Callback, Component, Context, Html, Properties};

use pdm_client::types::{ListController, ListVnet, ListZone};
use pwt::css;
use pwt::props::{ContainerBuilder, ExtractPrimaryKey, WidgetBuilder};
use pwt::state::{Selection, SlabTree, TreeStore};
use pwt::tr;
use pwt::widget::data_table::{
    DataTable, DataTableColumn, DataTableHeader, DataTableRowRenderArgs,
};
use pwt::widget::{error_message, Column, Fa, Row};
use pwt_macros::widget;

use crate::sdn::evpn::evpn_panel::DetailPanel;
use crate::sdn::evpn::EvpnRouteTarget;

#[widget(comp=VrfTreeComponent)]
#[derive(Clone, PartialEq, Properties, Default)]
pub struct VrfTree {
    zones: Rc<Vec<ListZone>>,
    vnets: Rc<Vec<ListVnet>>,
    controllers: Rc<Vec<ListController>>,
    on_select: Callback<Option<DetailPanel>>,
}

impl VrfTree {
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
struct VrfData {
    route_target: EvpnRouteTarget,
}

#[derive(Clone, PartialEq, Debug)]
struct FdbData {
    vrf_route_target: EvpnRouteTarget,
    route_target: EvpnRouteTarget,
}

#[derive(Clone, PartialEq, Debug)]
struct RemoteData {
    remote: String,
    zone: String,
    vnet: String,
}

#[derive(Clone, PartialEq, Debug)]
enum VrfTreeEntry {
    Root,
    Vrf(VrfData),
    Fdb(FdbData),
    Remote(RemoteData),
}

impl VrfTreeEntry {
    fn vni(&self) -> Option<u32> {
        match self {
            VrfTreeEntry::Vrf(vrf) => Some(vrf.route_target.vni),
            VrfTreeEntry::Fdb(fdb) => Some(fdb.route_target.vni),
            _ => None,
        }
    }

    fn asn(&self) -> Option<u32> {
        match self {
            VrfTreeEntry::Vrf(vrf) => Some(vrf.route_target.asn),
            _ => None,
        }
    }

    fn heading(&self) -> Option<String> {
        Some(match self {
            VrfTreeEntry::Root => return None,
            VrfTreeEntry::Vrf(_) => "IP-VRF".to_string(),
            VrfTreeEntry::Fdb(_) => "VNet".to_string(),
            VrfTreeEntry::Remote(remote) => remote.vnet.clone(),
        })
    }
}

impl ExtractPrimaryKey for VrfTreeEntry {
    fn extract_key(&self) -> Key {
        match self {
            Self::Root => Key::from("root"),
            Self::Vrf(vrf) => Key::from(vrf.route_target.to_string()),
            Self::Fdb(fdb) => Key::from(format!("{}/{}", fdb.vrf_route_target, fdb.route_target)),
            Self::Remote(remote) => {
                Key::from(format!("{}/{}/{}", remote.remote, remote.zone, remote.vnet,))
            }
        }
    }
}

fn zones_to_vrf_view(
    controllers: &[ListController],
    zones: &[ListZone],
    vnets: &[ListVnet],
) -> Result<SlabTree<VrfTreeEntry>, Error> {
    let mut tree = SlabTree::new();

    let mut root = tree.set_root(VrfTreeEntry::Root);
    root.set_expanded(true);

    let mut existing_vrfs: HashSet<EvpnRouteTarget> = HashSet::new();

    for zone in zones {
        let zone_data = &zone.zone;

        let zone_controller_id = zone_data.controller.as_ref().ok_or_else(|| {
            anyhow!(tr!(
                "EVPN zone {} has no controller defined!",
                &zone_data.zone
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
                    zone_data.zone
                ))
            })?;

        let controller_asn = controller.controller.asn.ok_or_else(|| {
            anyhow!(tr!(
                "EVPN controller {} has no ASN defined!",
                controller.controller.controller
            ))
        })?;

        let route_target = EvpnRouteTarget {
            asn: controller_asn,
            vni: zone
                .zone
                .vrf_vxlan
                .ok_or_else(|| anyhow!(tr!("EVPN Zone {} has no VRF VNI", zone_data.zone)))?,
        };

        if !existing_vrfs.insert(route_target) {
            continue;
        }

        let mut vrf_entry = root.append(VrfTreeEntry::Vrf(VrfData { route_target }));
        vrf_entry.set_expanded(true);
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

        for mut vrf_entry in root.children_mut() {
            if let VrfTreeEntry::Vrf(vrf_data) = vrf_entry.record() {
                if vrf_data.route_target != zone_target {
                    continue;
                }

                let searched_entry = vrf_entry.children_mut().find(|entry| {
                    if let VrfTreeEntry::Fdb(fdb_data) = entry.record() {
                        return fdb_data.route_target == vnet_target;
                    }

                    false
                });

                let mut fdb_entry = if let Some(fdb_entry) = searched_entry {
                    fdb_entry
                } else {
                    let fdb_entry = vrf_entry.append(VrfTreeEntry::Fdb(FdbData {
                        vrf_route_target: zone_target,
                        route_target: vnet_target,
                    }));

                    fdb_entry
                };

                let vnet_zone =
                    vnet.vnet.zone.as_ref().ok_or_else(|| {
                        anyhow!(tr!("VNet {} has no zone defined!", vnet.vnet.vnet))
                    })?;

                fdb_entry.append(VrfTreeEntry::Remote(RemoteData {
                    remote: vnet.remote.clone(),
                    zone: vnet_zone.clone(),
                    vnet: vnet.vnet.vnet.clone(),
                }));
            }
        }
    }

    Ok(tree)
}

pub struct VrfTreeComponent {
    store: TreeStore<VrfTreeEntry>,
    selection: Selection,
    error_msg: Option<String>,
    columns: Rc<Vec<DataTableHeader<VrfTreeEntry>>>,
}

fn default_sorter(a: &VrfTreeEntry, b: &VrfTreeEntry) -> Ordering {
    (a.asn(), a.vni()).cmp(&(b.asn(), b.vni()))
}

impl VrfTreeComponent {
    fn columns(store: TreeStore<VrfTreeEntry>) -> Rc<Vec<DataTableHeader<VrfTreeEntry>>> {
        Rc::new(vec![
            DataTableColumn::new(tr!("Type / Name"))
                .tree_column(store)
                .render(|item: &VrfTreeEntry| {
                    let heading = item.heading();

                    heading
                        .map(|heading| {
                            let mut row = Row::new().class(css::AlignItems::Baseline).gap(2);

                            row = match item {
                                VrfTreeEntry::Vrf(_) => row.with_child(Fa::new("th")),
                                VrfTreeEntry::Fdb(_) => row.with_child(Fa::new("sdn-vnet")),
                                _ => row,
                            };

                            row = row.with_child(heading);

                            Html::from(row)
                        })
                        .unwrap_or_default()
                })
                .sorter(default_sorter)
                .into(),
            DataTableColumn::new(tr!("ASN"))
                .render(|item: &VrfTreeEntry| item.asn().map(VNode::from).unwrap_or_default())
                .sorter(|a: &VrfTreeEntry, b: &VrfTreeEntry| a.asn().cmp(&b.asn()))
                .into(),
            DataTableColumn::new(tr!("VNI"))
                .render(|item: &VrfTreeEntry| item.vni().map(VNode::from).unwrap_or_default())
                .sorter(|a: &VrfTreeEntry, b: &VrfTreeEntry| a.vni().cmp(&b.vni()))
                .into(),
            DataTableColumn::new(tr!("Zone"))
                .get_property(|item: &VrfTreeEntry| match item {
                    VrfTreeEntry::Remote(remote) => remote.zone.as_str(),
                    _ => "",
                })
                .into(),
            DataTableColumn::new(tr!("Remote"))
                .get_property(|item: &VrfTreeEntry| match item {
                    VrfTreeEntry::Remote(remote) => remote.remote.as_str(),
                    _ => "",
                })
                .into(),
        ])
    }
}

impl Component for VrfTreeComponent {
    type Properties = VrfTree;
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
                        VrfTreeEntry::Remote(remote) => {
                            on_select.emit(Some(DetailPanel::Vnet {
                                remote: remote.remote.clone(),
                                vnet: remote.vnet.clone(),
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

        match zones_to_vrf_view(
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
        let table = DataTable::new(self.columns.clone(), self.store.clone())
            .striped(false)
            .selection(self.selection.clone())
            .row_render_callback(|args: &mut DataTableRowRenderArgs<VrfTreeEntry>| {
                if let VrfTreeEntry::Vrf(_) = args.record() {
                    args.add_class("pwt-bg-color-surface");
                }
            })
            .class(css::FlexFit);

        let mut column = Column::new().class(pwt::css::FlexFit).with_child(table);

        if let Some(msg) = &self.error_msg {
            column.add_child(error_message(msg.as_ref()));
        }

        column.into()
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        if !Rc::ptr_eq(&ctx.props().zones, &old_props.zones)
            || !Rc::ptr_eq(&ctx.props().vnets, &old_props.vnets)
            || !Rc::ptr_eq(&ctx.props().controllers, &old_props.controllers)
        {
            match zones_to_vrf_view(
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
