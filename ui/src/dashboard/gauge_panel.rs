use anyhow::Error;

use proxmox_human_byte::HumanByte;
use pwt::css;
use pwt::prelude::*;
use pwt::state::SharedState;
use pwt::widget::Fa;
use pwt::widget::{charts::PieChart, Panel};
use pwt::widget::{error_message, Column, Container, Row};

use pdm_api_types::remotes::RemoteType;
use pdm_api_types::{resource::ResourcesStatus, views::NodeResourceType};

use crate::dashboard::{create_title_with_icon, loading_column};
use crate::LoadResult;

struct PanelConfig {
    show_cpu: bool,
    show_mem: bool,
    show_storage: bool,
    title: String,
    subtitle: bool,
    icon: &'static str,
}

impl PanelConfig {
    fn new(resource_type: Option<NodeResourceType>) -> Self {
        match resource_type {
            Some(NodeResourceType::Cpu) => PanelConfig {
                show_cpu: true,
                show_mem: false,
                show_storage: false,
                title: tr!("CPU Usage"),
                subtitle: false,
                icon: "cpu",
            },
            Some(NodeResourceType::Memory) => PanelConfig {
                show_cpu: false,
                show_mem: true,
                show_storage: false,
                title: tr!("Memory Usage"),
                subtitle: false,
                icon: "memory",
            },
            Some(NodeResourceType::Storage) => PanelConfig {
                show_cpu: false,
                show_mem: false,
                show_storage: true,
                title: tr!("Storage Usage"),
                subtitle: false,
                icon: "database",
            },
            None => PanelConfig {
                show_cpu: true,
                show_mem: true,
                show_storage: true,
                title: tr!("Resource Usage"),
                subtitle: true,
                icon: "tachometer",
            },
        }
    }
}

/// Creates a new gauge chart panel. Setting `resource_type` to `None` means we
/// create one gauge chart for each type in one panel.
///
/// Using `None` as remote_type means the resources from PVE and PBS will be combined.
pub fn create_gauge_panel(
    resource_type: Option<NodeResourceType>,
    remote_type: Option<RemoteType>,
    status: SharedState<LoadResult<ResourcesStatus, Error>>,
) -> Panel {
    let status = status.read();
    let conf = PanelConfig::new(resource_type);

    let suffix = match remote_type {
        Some(RemoteType::Pve) => " - Virtual Environment",
        Some(RemoteType::Pbs) => " - Backup Server",
        None => "",
    };

    let is_loading = !status.has_data();

    Panel::new()
        .title(create_title_with_icon(
            conf.icon,
            format!("{}{suffix}", conf.title),
        ))
        .border(true)
        .with_optional_child(status.data.as_ref().map(|data| {
            let (cpu, mem, storage) = match remote_type {
                Some(RemoteType::Pve) => (
                    conf.show_cpu
                        .then_some((data.pve_cpu_stats.used, data.pve_cpu_stats.max)),
                    conf.show_mem
                        .then_some((data.pve_memory_stats.used, data.pve_memory_stats.total)),
                    conf.show_storage
                        .then_some((data.pve_storage_stats.used, data.pve_storage_stats.total)),
                ),
                Some(RemoteType::Pbs) => (
                    conf.show_cpu
                        .then_some((data.pbs_cpu_stats.used, data.pbs_cpu_stats.max)),
                    conf.show_mem
                        .then_some((data.pbs_memory_stats.used, data.pbs_memory_stats.total)),
                    conf.show_storage
                        .then_some((data.pbs_storage_stats.used, data.pbs_storage_stats.total)),
                ),
                None => (
                    conf.show_cpu.then_some((
                        data.pve_cpu_stats.used + data.pbs_cpu_stats.used,
                        data.pve_cpu_stats.max + data.pbs_cpu_stats.max,
                    )),
                    conf.show_mem.then_some((
                        data.pve_memory_stats.used + data.pbs_memory_stats.used,
                        data.pve_memory_stats.total + data.pbs_memory_stats.total,
                    )),
                    conf.show_storage.then_some((
                        data.pve_storage_stats.used + data.pbs_storage_stats.used,
                        data.pve_storage_stats.total + data.pbs_storage_stats.total,
                    )),
                ),
            };

            let chart = |percentage: f64, icon: Fa, title: String, extra_text: String| -> Column {
                let subtitle = conf.subtitle.then_some(
                    Row::new()
                        .gap(1)
                        .class(css::AlignItems::Center)
                        .class(css::JustifyContent::Center)
                        .with_child(icon)
                        .with_child(&title),
                );
                Column::new()
                    .flex(1.0)
                    .width("0") // correct flex base size for calculation
                    .max_height(250)
                    .with_child(
                        PieChart::gauge(title, percentage)
                            .class(css::Overflow::Auto)
                            .text(format!("{:.0}%", percentage * 100.))
                            .angle_start(75.0)
                            .angle_end(285.0)
                            .show_tooltip(false),
                    )
                    .with_optional_child(subtitle)
                    .with_child(
                        Container::new()
                            .padding_top(1)
                            .class(css::TextAlign::Center)
                            .with_child(extra_text),
                    )
            };

            Row::new()
                .padding(4)
                .with_optional_child(cpu.map(|(used, total)| {
                    let pct = if total == 0.0 { 0.0 } else { used / total };
                    let extra_text = match remote_type {
                        Some(RemoteType::Pve) => {
                            tr!(
                                "{0} of {1} cores ({2} allocated)",
                                format!("{used:.2}"),
                                format!("{total:.0}"),
                                format!("{:.0}", data.pve_cpu_stats.allocated.unwrap_or(0.0)),
                            )
                        }
                        _ => {
                            tr!(
                                "{0} of {1} cores",
                                format!("{used:.2}"),
                                format!("{total:.0}")
                            )
                        }
                    };
                    chart(pct, Fa::new("cpu"), tr!("CPU"), extra_text)
                }))
                .with_optional_child(mem.map(|(used, total)| {
                    chart(
                        if total == 0 {
                            0.0
                        } else {
                            used as f64 / total as f64
                        },
                        Fa::new("memory"),
                        tr!("Memory"),
                        tr!("{0} of {1}", HumanByte::from(used), HumanByte::from(total)),
                    )
                }))
                .with_optional_child(storage.map(|(used, total)| {
                    chart(
                        if total == 0 {
                            0.0
                        } else {
                            used as f64 / total as f64
                        },
                        Fa::new("database"),
                        tr!("Storage"),
                        tr!("{0} of {1}", HumanByte::from(used), HumanByte::from(total)),
                    )
                }))
        }))
        .with_optional_child(is_loading.then_some(loading_column()))
        .with_optional_child(
            status
                .error
                .as_ref()
                .map(|err| error_message(&err.to_string())),
        )
}
