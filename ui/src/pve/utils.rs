use pdm_api_types::resource::{
    PveLxcResource, PveNodeResource, PveQemuResource, PveStorageResource,
};
use pdm_client::types::Remote;
use proxmox_yew_comp::{GuestState, NodeState, StorageState};
use pwt::{
    css::Opacity,
    props::{ContainerBuilder, WidgetBuilder, WidgetStyleBuilder},
    widget::{Container, Fa, Row},
};

use crate::RemoteList;

/// Renders the display name for Virtual Machines, e.g. used for resource trees
pub fn render_qemu_name(qemu: &PveQemuResource, vmid_first: bool) -> String {
    render_guest_name(&qemu.name, qemu.vmid, vmid_first)
}

/// Renders the display name for Linux Containers, e.g. used for resource trees
pub fn render_lxc_name(lxc: &PveLxcResource, vmid_first: bool) -> String {
    render_guest_name(&lxc.name, lxc.vmid, vmid_first)
}

fn render_guest_name(name: &str, vmid: u32, vmid_first: bool) -> String {
    if vmid_first {
        format!("{vmid} ({name})")
    } else {
        format!("{name} ({vmid})")
    }
}

/// Get the global remote list if loaded
pub fn get_remote_list<C: yew::Component>(link: &yew::html::Scope<C>) -> Option<RemoteList> {
    let (list, _) = link.context(yew::Callback::from(|_: RemoteList| {}))?;
    Some(list)
}

/// Get the given remote from the global remote list if loaded
pub fn get_remote<C: yew::Component>(link: &yew::html::Scope<C>, id: &str) -> Option<Remote> {
    for remote in get_remote_list(link)?.iter() {
        if remote.id == id {
            return Some(remote.clone());
        }
    }

    None
}

/// Renders the status icon for a Virtual Machine
pub fn render_lxc_status_icon(lxc: &PveLxcResource) -> Container {
    render_guest_status_icon("cube", &lxc.status, lxc.template)
}

/// Renders the status icon for a Virtual Machine
pub fn render_qemu_status_icon(qemu: &PveQemuResource) -> Container {
    render_guest_status_icon("desktop", &qemu.status, qemu.template)
}

fn render_guest_status_icon(base: &str, status: &str, template: bool) -> Container {
    let (status, extra_class) = match (status, template) {
        ("running", _) => (
            Some(
                GuestState::Running
                    .to_fa_icon()
                    .fixed_width()
                    .class("status-icon"),
            ),
            None,
        ),
        ("stopped", false) => (None, Some(Opacity::Quarter)),
        (_, true) => (Some(Fa::new(base).fixed_width().class("status-icon")), None),
        _ => (Some(GuestState::Unknown.to_fa_icon()), None),
    };
    Container::new()
        .class("pve-guest-icon")
        .with_child(
            Fa::new(if template { "file-o" } else { base })
                .fixed_width()
                .class(extra_class),
        )
        .with_optional_child(status)
}

/// Renders the status icon for a PveNode
pub fn render_node_status_icon(node: &PveNodeResource) -> Container {
    let extra = match node.status.as_str() {
        "online" => NodeState::Online,
        "offline" => NodeState::Offline,
        _ => NodeState::Unknown,
    };
    Container::new()
        .class("pdm-type-icon")
        .with_child(Fa::new("building").fixed_width())
        .with_child(extra.to_fa_icon().fixed_width().class("status-icon"))
}

/// Renders the status icon for a PveStorage
pub fn render_storage_status_icon(node: &PveStorageResource) -> Container {
    let extra = match node.status.as_str() {
        "available" => StorageState::Available,
        _ => StorageState::Unknown,
    };
    Container::new()
        .class("pdm-type-icon")
        .with_child(Fa::new("database").fixed_width())
        .with_child(extra.to_fa_icon().fixed_width().class("status-icon"))
}

/// Returns a [`pwt::widget::Row`] with an element for each tag
pub fn render_guest_tags(tags: &[String]) -> Row {
    let mut row = Row::new().class("pve-tags").gap(2);

    for tag in tags {
        if tag.is_empty() {
            continue;
        }
        let color = pdm_ui_shared::colors::text_to_rgb(tag).unwrap();
        let foreground = pdm_ui_shared::colors::get_best_contrast_color(&color);

        row.add_child(
            Container::new()
                .class("pve-tag")
                .style("background-color", color.as_css_rgb().to_string())
                .style("color", foreground.as_css_rgb().to_string())
                .with_child(tag),
        );
    }
    row
}