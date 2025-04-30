use std::rc::Rc;

use proxmox_yew_comp::Status;
use yew::Properties;

use proxmox_human_byte::HumanByte;
use pwt::{
    css::{AlignItems, FontStyle},
    prelude::*,
    props::WidgetBuilder,
    widget::{error_message, Column, Fa, Panel, Row},
};
use pwt_macros::widget;

use pdm_api_types::resource::PveResource;

use crate::renderer::separator;

#[widget(comp=RemotePanelComp, @element)]
#[derive(Clone, Debug, PartialEq, Properties)]
pub struct RemotePanel {
    remote: String,
    resources: Rc<Vec<PveResource>>,
    error: Option<String>,
}

impl RemotePanel {
    pub fn new(remote: String, resources: Rc<Vec<PveResource>>, error: Option<String>) -> Self {
        yew::props!(Self {
            remote,
            resources,
            error
        })
    }
}

pub struct RemotePanelComp {
    status: RemoteStatus,
}

#[derive(Default)]
struct RemoteStatus {
    guests: i32,
    guests_running: i32,
    guest_cores_running: f64,
    guest_cores: f64,
    guest_memory_running: u64,
    guest_memory: u64,
    max_cores: f64,
    memory: u64,
    max_memory: u64,
    storage: u64,
    max_storage: u64,
    nodes: i32,
    cpu_usage: f64,
    level: &'static str,
}

impl RemotePanelComp {
    fn calculate_status(&mut self, ctx: &yew::Context<Self>) {
        let mut guests = 0;
        let mut guests_running = 0;
        let mut guest_cores_running = 0.0;
        let mut guest_cores = 0.0;
        let mut guest_memory_running = 0;
        let mut guest_memory = 0;
        let mut max_cores = 0.0;
        let mut memory = 0;
        let mut max_memory = 0;
        let mut storage = 0;
        let mut max_storage = 1;
        let mut nodes = 0;
        let mut cpu_usage = 0.0;
        let mut level = None;

        for res in ctx.props().resources.iter() {
            match res {
                PveResource::Storage(store) => {
                    storage += store.disk;
                    max_storage += store.maxdisk;
                }
                PveResource::Qemu(qemu) => {
                    guests += 1;
                    guest_cores += qemu.maxcpu;
                    guest_memory += qemu.maxmem;
                    if qemu.status != "stopped" {
                        guests_running += 1;
                        guest_cores_running += qemu.maxcpu;
                        guest_memory_running += qemu.maxmem;
                    }
                }
                PveResource::Lxc(lxc) => {
                    guests += 1;
                    guest_cores += lxc.maxcpu;
                    guest_memory += lxc.maxmem;
                    if lxc.status != "stopped" {
                        guests_running += 1;
                        guest_cores_running += lxc.maxcpu;
                        guest_memory_running += lxc.maxmem;
                    }
                }
                PveResource::Node(node) => {
                    nodes += 1;
                    max_cores += node.maxcpu;
                    memory += node.mem;
                    max_memory += node.maxmem;
                    cpu_usage += node.cpu;

                    match (node.level.as_str(), level) {
                        (x, Some(y)) if x == y => {}
                        (x, Some(y)) if x != y => {
                            level = Some(""); // TODO: add separate state for all subscribed but mixed.
                        }
                        ("c" | "b" | "s" | "p", None) => level = Some(node.level.as_str()),
                        _ => level = Some(""),
                    }
                }
            }
        }
        // render, but this would be all better with some actual types...
        let level = match level {
            Some("c") => "Community",
            Some("b") => "Basic",
            Some("s") => "Standard",
            Some("p") => "Premium",
            _ => "",
        };

        let cpu_usage = cpu_usage / nodes as f64;

        self.status = RemoteStatus {
            guests,
            guests_running,
            guest_cores_running,
            guest_cores,
            guest_memory_running,
            guest_memory,
            max_cores,
            memory,
            max_memory,
            storage,
            max_storage,
            nodes,
            cpu_usage,
            level,
        };
    }
}

impl yew::Component for RemotePanelComp {
    type Message = ();

    type Properties = RemotePanel;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self {
            status: Default::default(),
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        if ctx.props().resources != old_props.resources {
            self.calculate_status(ctx);
        }
        true
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let status = &self.status;
        let content = match &ctx.props().error {
            Some(err) => Column::new().padding(4).with_child(error_message(err)),
            None => Column::new()
                .gap(2)
                .with_child(make_row(
                    tr!("Subscription Status"),
                    if status.level.is_empty() {
                        Status::Error.into()
                    } else {
                        Status::Success.into()
                    },
                    status.level.to_string(),
                    None,
                ))
                .with_child(make_row(
                    tr! {"Nodes"},
                    Fa::new("building"),
                    format!("{}", status.nodes),
                    None,
                ))
                .with_child(make_row(
                    tr! {"Guests"},
                    Fa::new("desktop"),
                    tr!(
                        "{0} / {1} (running / total)",
                        status.guests_running,
                        status.guests
                    ),
                    None,
                ))
                .with_child(separator())
                .with_child(
                    Row::new()
                        .class(AlignItems::Baseline)
                        .class(FontStyle::TitleMedium)
                        .gap(2)
                        .with_child(Fa::new("bar-chart"))
                        .with_child(tr!("Usage")),
                )
                .with_child(make_row(
                    tr! {"Host CPU usage (avg.)"},
                    Fa::new("cpu"),
                    format!("{:.2}%", status.cpu_usage * 100.0),
                    Some(status.cpu_usage as f32),
                ))
                .with_child(make_row(
                    tr! {"Host Memory used"},
                    Fa::new("memory"),
                    tr!(
                        "{0}% ({1} of {2})",
                        format!(
                            "{:.2}",
                            100.0 * status.memory as f64 / status.max_memory as f64
                        ),
                        HumanByte::from(status.memory),
                        HumanByte::from(status.max_memory),
                    ),
                    Some((status.memory as f64 / status.max_memory as f64) as f32),
                ))
                .with_child(make_row(
                    tr! {"Host Storage used"},
                    Fa::new("database"),
                    tr!(
                        "{0}% ({1} of {2})",
                        format!(
                            "{:.2}",
                            100.0 * status.storage as f64 / status.max_storage as f64
                        ),
                        HumanByte::from(status.storage),
                        HumanByte::from(status.max_storage)
                    ),
                    Some((status.storage as f64 / status.max_storage as f64) as f32),
                ))
                .with_child(separator())
                .with_child(
                    Row::new()
                        .class(AlignItems::Baseline)
                        .class(FontStyle::TitleMedium)
                        .gap(2)
                        .with_child(Fa::new("pie-chart"))
                        .with_child(tr!("Allocation")),
                )
                .with_child(make_row(
                    tr! {"CPU Cores assigned"},
                    Fa::new("cpu"),
                    tr!(
                        "{0} running / {1} physical ({2} total configured)",
                        status.guest_cores_running,
                        status.max_cores,
                        status.guest_cores,
                    ),
                    None,
                ))
                .with_child(make_row(
                    tr! {"Memory assigned"},
                    Fa::new("memory"),
                    tr!(
                        "{0} running / {1} physical ({2} total configured)",
                        HumanByte::from(status.guest_memory_running),
                        HumanByte::from(status.max_memory),
                        HumanByte::from(status.guest_memory),
                    ),
                    None,
                )),
        };

        Panel::new()
            .with_std_props(&ctx.props().std_props)
            .with_child(content)
            .into()
    }
}

fn make_row(title: String, icon: Fa, text: String, meter_value: Option<f32>) -> Column {
    crate::renderer::status_row(title, icon, text, meter_value, true)
}
