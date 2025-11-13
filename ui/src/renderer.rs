use proxmox_yew_comp::MeterLabel;
use pwt::{
    css::AlignItems,
    prelude::*,
    props::ContainerBuilder,
    widget::{Container, Fa, Row},
};

use proxmox_human_byte::HumanByte;

use pdm_client::types::Resource;

use crate::pve;

pub fn render_resource_name(resource: &Resource, vmid_first: bool) -> String {
    match resource {
        Resource::PveStorage(storage) => storage.storage.clone(),
        Resource::PveQemu(qemu) => pve::utils::render_qemu_name(qemu, vmid_first),
        Resource::PveLxc(lxc) => pve::utils::render_lxc_name(lxc, vmid_first),
        Resource::PveNode(node) => node.node.clone(),
        Resource::PveNetwork(network) => network.name().to_string(),
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
        Resource::PveNetwork(_) => "fa-sdn",
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
        Resource::PveNetwork(network) => pve::utils::render_sdn_status_icon(network),
        // FIXME: implement remaining types
        _ => Container::new().with_child(render_resource_icon(resource)),
    }
}

pub(crate) fn status_row_right_icon(
    title: String,
    icon: impl Into<Classes>,
    text: String,
) -> MeterLabel {
    status_row(title, icon, text).icon_right(true)
}

pub(crate) fn status_row(title: String, icon: impl Into<Classes>, text: String) -> MeterLabel {
    MeterLabel::with_zero_optimum(title)
        .icon_class(classes!(icon.into(), "fa", "fa-fw"))
        .low(0.8)
        .high(0.9)
        .animated(true)
        .status(text)
}

pub(crate) fn memory_status_row(used: u64, total: u64) -> MeterLabel {
    let usage = used as f64 / total as f64;
    status_row(
        tr!("Memory usage"),
        "fa-memory",
        tr!(
            "{0}% ({1} of {2})",
            format!("{:.2}", usage * 100.0),
            HumanByte::from(used),
            HumanByte::from(total),
        ),
    )
    .low(0.9)
    .high(0.975)
    .value(usage as f32)
}

pub(crate) fn separator() -> Container {
    Container::new().with_child(html! {<hr />}).padding_y(2)
}

pub(crate) fn render_tree_column(icon: Html, text: String) -> Row {
    Row::new()
        .min_width(0)
        .class(AlignItems::Center)
        .gap(2)
        .with_child(icon)
        .with_child(
            Container::new()
                .with_child(text)
                .style("text-overflow", "ellipsis")
                .style("overflow", "hidden"),
        )
}
