use pwt::{
    css,
    prelude::*,
    props::ContainerBuilder,
    widget::{Column, Container, Fa, Meter, Row},
};

use pdm_client::types::Resource;

use crate::pve;

pub fn render_resource_name(resource: &Resource, vmid_first: bool) -> String {
    match resource {
        Resource::PveStorage(storage) => storage.storage.clone(),
        Resource::PveQemu(qemu) => pve::utils::render_qemu_name(qemu, vmid_first),
        Resource::PveLxc(lxc) => pve::utils::render_lxc_name(lxc, vmid_first),
        Resource::PveNode(node) => node.node.clone(),
        Resource::PbsNode(node) => node.name.clone(),
        Resource::PbsDatastore(store) => store.name.clone(),
    }
}

pub fn render_resource_icon(resource: &Resource) -> Fa {
    let class = match resource {
        Resource::PveStorage(_) => "database",
        Resource::PveQemu(_) => "desktop",
        Resource::PveLxc(_) => "cube",
        Resource::PveNode(_) => "building",
        Resource::PbsNode(_) => "building-o",
        Resource::PbsDatastore(_) => "floppy-o",
    };

    Fa::new(class)
}

pub fn render_status_icon(resource: &Resource) -> Container {
    match resource {
        Resource::PveStorage(store) => pve::utils::render_storage_status_icon(store),
        Resource::PveQemu(qemu) => pve::utils::render_qemu_status_icon(qemu),
        Resource::PveLxc(lxc) => pve::utils::render_lxc_status_icon(lxc),
        Resource::PveNode(node) => pve::utils::render_node_status_icon(node),
        // FIXME: implement remaining types
        _ => Container::new().with_child(render_resource_icon(resource)),
    }
}

pub(crate) fn status_row(
    title: String,
    icon: Fa,
    text: String,
    meter_value: Option<f32>,
    icon_right: bool,
) -> Column {
    let row = Row::new()
        .class(css::AlignItems::Baseline)
        .gap(2)
        .with_optional_child((!icon_right).then_some(icon.clone().fixed_width()))
        .with_child(title)
        .with_flex_spacer()
        .with_child(text)
        .with_optional_child((icon_right).then_some(icon.fixed_width()));

    Column::new()
        .gap(1)
        .with_child(row)
        .with_optional_child(meter_value.map(|value| {
            Meter::new()
                .optimum(0.0)
                .low(0.7)
                .high(0.9)
                .animated(true)
                .value(value)
        }))
}

pub(crate) fn separator() -> Container {
    Container::new().with_child(html! {<hr />}).padding_y(2)
}
