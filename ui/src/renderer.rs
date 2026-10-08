use proxmox_yew_comp::MeterLabel;
use pwt::css::{AlignItems, FlexFit, FontStyle, JustifyContent};
use pwt::prelude::*;
use pwt::props::ContainerBuilder;
use pwt::widget::{Badge, Column, Container, Fa, Row};

use proxmox_human_byte::HumanByte;

use pdm_api_types::resource::{AsResourceView, PveNodeHaState, ResourceView};

use crate::pve;
use crate::pve::utils::render_guest_tags;

pub fn render_resource_name(resource: impl AsResourceView, vmid_first: bool) -> String {
    match resource.as_resource_view() {
        ResourceView::PveStorage(storage) => storage.storage.clone(),
        ResourceView::PveQemu(qemu) => pve::utils::render_qemu_name(qemu, vmid_first),
        ResourceView::PveLxc(lxc) => pve::utils::render_lxc_name(lxc, vmid_first),
        ResourceView::PveNode(node) => node.node.clone(),
        ResourceView::PveNetwork(network) => network.name().to_string(),
        ResourceView::PbsNode(node) => node.name.clone(),
        ResourceView::PbsDatastore(store) => store.name.clone(),
    }
}

pub fn render_resource_extra_info(resource: impl AsResourceView) -> Option<Html> {
    match resource.as_resource_view() {
        ResourceView::PveNode(node) => match node.ha_state.as_ref()? {
            PveNodeHaState::Maintenance => Some(
                Badge::new(tr!("HA Maintenance"))
                    .color_scheme(pwt::css::ColorScheme::PrimaryContainer)
                    .icon("wrench")
                    .into(),
            ),
            _ => None,
        },
        ResourceView::PveQemu(q) if !q.tags.is_empty() => Some(render_guest_tags(&q.tags).into()),
        ResourceView::PveLxc(l) if !l.tags.is_empty() => Some(render_guest_tags(&l.tags).into()),
        _ => None,
    }
}

pub fn render_resource_icon(resource: impl AsResourceView) -> Fa {
    let class = match resource.as_resource_view() {
        ResourceView::PveStorage(_) => "database",
        ResourceView::PveQemu(_) => "desktop",
        ResourceView::PveLxc(_) => "cube",
        ResourceView::PveNode(_) => "building",
        ResourceView::PveNetwork(_) => "fa-sdn",
        ResourceView::PbsNode(_) => "building-o",
        ResourceView::PbsDatastore(_) => "floppy-o",
    };

    Fa::new(class)
}

pub fn render_status_icon(resource: impl AsResourceView) -> Container {
    match resource.as_resource_view() {
        ResourceView::PveStorage(store) => pve::utils::render_storage_status_icon(store),
        ResourceView::PveQemu(qemu) => pve::utils::render_qemu_status_icon(qemu),
        ResourceView::PveLxc(lxc) => pve::utils::render_lxc_status_icon(lxc),
        ResourceView::PveNode(node) => pve::utils::render_node_status_icon(node),
        ResourceView::PveNetwork(network) => pve::utils::render_sdn_status_icon(network),
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

/// Returns a simple row with a title and an icon that can be used to separate sections.
pub(crate) fn render_title_row(title: String, icon: &str) -> Row {
    Row::new()
        .class(pwt::css::AlignItems::Baseline)
        .class(pwt::css::FontStyle::TitleMedium)
        .gap(2)
        .with_child(Fa::new(icon))
        .with_child(title)
}

/// A centered, large-icon empty state with a title and an explanatory hint.
pub(crate) fn empty_state(icon: &str, title: String, hint: String) -> Html {
    Column::new()
        .class(FlexFit)
        .class(JustifyContent::Center)
        .class(AlignItems::Center)
        .gap(2)
        .padding(4)
        .with_child(Fa::new(icon).large_3x())
        .with_child(
            Container::from_tag("span")
                .class(FontStyle::TitleMedium)
                .with_child(title),
        )
        .with_child(Container::from_tag("span").with_child(hint))
        .into()
}
