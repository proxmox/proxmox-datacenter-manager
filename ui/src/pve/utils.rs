use anyhow::Error;
use pdm_api_types::resource::{
    PveLxcResource, PveNodeResource, PveQemuResource, PveStorageResource,
};
use pdm_client::types::{
    LxcConfig, LxcConfigMp, LxcConfigRootfs, LxcConfigUnused, PveQmIde, QemuConfig, QemuConfigSata,
    QemuConfigScsi, QemuConfigUnused, QemuConfigVirtio, Remote,
};
use proxmox_schema::ApiType;
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
        ("running", false) => (
            Some(
                GuestState::Running
                    .to_fa_icon()
                    .fixed_width()
                    .class("status-icon"),
            ),
            None,
        ),
        ("stopped", false) => (None, Some(Opacity::Quarter)),
        ("paused", false) => (
            Some(
                GuestState::Paused
                    .to_fa_icon()
                    .fixed_width()
                    .class("status-icon"),
            ),
            None,
        ),
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

/// Represents a drive of a QEMU guest
pub enum PveDriveQemu {
    Sata(QemuConfigSata),
    Scsi(QemuConfigScsi),
    Ide(PveQmIde),
    Virtio(QemuConfigVirtio),
    Unused(QemuConfigUnused),
}

impl PveDriveQemu {
    /// Returns the file of the drive, regardless of bus
    pub fn get_file(&self) -> &str {
        match self {
            PveDriveQemu::Sata(QemuConfigSata { file, .. })
            | PveDriveQemu::Scsi(QemuConfigScsi { file, .. })
            | PveDriveQemu::Ide(PveQmIde { file, .. })
            | PveDriveQemu::Virtio(QemuConfigVirtio { file, .. })
            | PveDriveQemu::Unused(QemuConfigUnused { file, .. }) => file,
        }
    }
}

// note: uses to_value so we can iterate over the keys
// unwrap is ok here, since we know that those are structs and strings
/// Iterates over every drive from the config
pub fn foreach_drive_qemu<F>(config: &QemuConfig, mut f: F) -> Result<(), Error>
where
    F: FnMut(&str, PveDriveQemu),
{
    let ide = serde_json::to_value(&config.ide)?;
    for (key, value) in ide.as_object().unwrap() {
        let value = serde_json::from_value(
            PveQmIde::API_SCHEMA.parse_property_string(value.as_str().unwrap())?,
        )?;

        f(key, PveDriveQemu::Ide(value))
    }

    let sata = serde_json::to_value(&config.sata)?;
    for (key, value) in sata.as_object().unwrap() {
        let value = serde_json::from_value(
            QemuConfigSata::API_SCHEMA.parse_property_string(value.as_str().unwrap())?,
        )?;

        f(key, PveDriveQemu::Sata(value))
    }

    let scsi = serde_json::to_value(&config.scsi)?;
    for (key, value) in scsi.as_object().unwrap() {
        let value = serde_json::from_value(
            QemuConfigScsi::API_SCHEMA.parse_property_string(value.as_str().unwrap())?,
        )?;

        f(key, PveDriveQemu::Scsi(value))
    }

    let virtio = serde_json::to_value(&config.virtio)?;
    for (key, value) in virtio.as_object().unwrap() {
        let value = serde_json::from_value(
            QemuConfigVirtio::API_SCHEMA.parse_property_string(value.as_str().unwrap())?,
        )?;

        f(key, PveDriveQemu::Virtio(value))
    }

    let unused = serde_json::to_value(&config.unused)?;
    for (key, value) in unused.as_object().unwrap() {
        let value = serde_json::from_value(
            QemuConfigUnused::API_SCHEMA.parse_property_string(value.as_str().unwrap())?,
        )?;

        f(key, PveDriveQemu::Unused(value))
    }

    Ok(())
}

/// Represents a drive from an LXC config
pub enum PveDriveLxc {
    RootFs(LxcConfigRootfs),
    Mp(LxcConfigMp),
    Unused(LxcConfigUnused),
}

impl PveDriveLxc {
    /// Returns the volume of the drive, regardless of type
    pub fn get_volume(&self) -> &str {
        match self {
            PveDriveLxc::RootFs(LxcConfigRootfs { volume, .. })
            | PveDriveLxc::Mp(LxcConfigMp { volume, .. })
            | PveDriveLxc::Unused(LxcConfigUnused { volume }) => volume,
        }
    }
}

/// Iterates over every drive from the config
pub fn foreach_drive_lxc<F>(config: &LxcConfig, mut f: F) -> Result<(), Error>
where
    F: FnMut(&str, PveDriveLxc),
{
    if let Some(rootfs) = &config.rootfs {
        let value =
            serde_json::from_value(LxcConfigRootfs::API_SCHEMA.parse_property_string(&rootfs)?)?;

        f("rootfs", PveDriveLxc::RootFs(value))
    }

    let mp = serde_json::to_value(&config.mp)?;
    for (key, value) in mp.as_object().unwrap() {
        let value = serde_json::from_value(
            LxcConfigMp::API_SCHEMA.parse_property_string(value.as_str().unwrap())?,
        )?;

        f(key, PveDriveLxc::Mp(value))
    }

    let unused = serde_json::to_value(&config.unused)?;
    for (key, value) in unused.as_object().unwrap() {
        let value = serde_json::from_value(
            LxcConfigUnused::API_SCHEMA.parse_property_string(value.as_str().unwrap())?,
        )?;

        f(key, PveDriveLxc::Unused(value))
    }

    Ok(())
}
