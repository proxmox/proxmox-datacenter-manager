use std::rc::Rc;

use yew::Properties;

use proxmox_human_byte::HumanByte;
use pwt::{
    css::AlignItems,
    prelude::*,
    props::WidgetBuilder,
    widget::{error_message, Column, Fa, Meter, Panel, Row},
};
use pwt_macros::widget;

use pdm_api_types::resource::PveResource;

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
    memory_usage: f64,
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
        let mut memory_usage = 0.0;
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
                    if qemu.status == "stopped" {
                        guest_cores += qemu.maxcpu;
                        guest_memory += qemu.maxmem;
                    } else {
                        guests_running += 1;
                        guest_cores_running += qemu.maxcpu;
                        guest_memory_running += qemu.maxmem;
                    }
                }
                PveResource::Lxc(lxc) => {
                    guests += 1;
                    if lxc.status == "stopped" {
                        guest_cores += lxc.maxcpu;
                        guest_memory += lxc.maxmem;
                    } else {
                        guest_cores_running += lxc.maxcpu;
                        guest_memory_running += lxc.maxmem;
                    }
                }
                PveResource::Node(node) => {
                    nodes += 1;
                    max_cores += node.maxcpu;
                    max_memory += node.maxmem;
                    cpu_usage += node.cpu;

                    memory_usage += node.mem as f64 / node.maxmem as f64;

                    match (node.level.as_str(), level) {
                        (x, Some(y)) if x == y => {}
                        (x, Some(y)) if x != y => {
                            level = Some("");
                        }
                        ("c", None) => level = Some("Community"),
                        ("b", None) => level = Some("Basic"),
                        ("s", None) => level = Some("Standard"),
                        ("p", None) => level = Some("Premium"),
                        _ => level = Some(""),
                    }
                }
            }
        }

        let cpu_usage = cpu_usage / nodes as f64;
        let memory_usage = memory_usage / nodes as f64;

        self.status = RemoteStatus {
            guests,
            guests_running,
            guest_cores_running,
            guest_cores,
            guest_memory_running,
            guest_memory,
            max_cores,
            memory_usage,
            max_memory,
            storage,
            max_storage,
            nodes,
            cpu_usage,
            level: level.unwrap_or_default(),
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
                .gap(4)
                .with_child(make_row(
                    tr!("Subscription Status"),
                    if status.level.is_empty() {
                        "times-circle"
                    } else {
                        "check"
                    },
                    status.level.to_string(),
                    None,
                ))
                .with_child(make_row(
                    tr! {"Nodes"},
                    "building",
                    format!("{}", status.nodes),
                    None,
                ))
                .with_child(make_row(
                    tr! {"Guests running"},
                    "desktop",
                    tr!("{0} ({1} total)", status.guests_running, status.guests),
                    None,
                ))
                .with_child(make_row(
                    tr! {"Host Cores assigned"},
                    "database",
                    tr!(
                        "{0} / {1} ({2} total configured)",
                        status.guest_cores_running,
                        status.max_cores,
                        status.guest_cores,
                    ),
                    None,
                ))
                .with_child(make_row(
                    tr! {"Host CPU usage (avg.)"},
                    "database",
                    format!("{:.2}%", status.cpu_usage * 100.0),
                    Some(status.cpu_usage as f32),
                ))
                .with_child(make_row(
                    tr! {"Host Memory assigned"},
                    "list",
                    tr!(
                        "{0} / {1} ({2} total)",
                        HumanByte::from(status.guest_memory_running),
                        HumanByte::from(status.max_memory),
                        HumanByte::from(status.guest_memory),
                    ),
                    Some((status.guest_memory_running as f64 / status.max_memory as f64) as f32),
                ))
                .with_child(make_row(
                    tr! {"Host Memory usage (avg.)"},
                    "list",
                    format!("{:.2}%", status.memory_usage * 100.0),
                    Some(status.memory_usage as f32),
                ))
                .with_child(make_row(
                    tr! {"Host Storage used"},
                    "server",
                    format!(
                        "{} / {}",
                        HumanByte::from(status.storage),
                        HumanByte::from(status.max_storage)
                    ),
                    Some((status.storage as f64 / status.max_storage as f64) as f32),
                )),
        };

        Panel::new()
            .with_std_props(&ctx.props().std_props)
            .with_child(content)
            .into()
    }
}

fn make_row(title: String, icon: &str, text: String, meter_value: Option<f32>) -> Column {
    let row = Row::new()
        .class(AlignItems::Baseline)
        //.class(FontStyle::HeadlineSmall)
        .gap(2)
        .with_child(title)
        .with_flex_spacer()
        .with_child(text)
        .with_child(Fa::new(icon).fixed_width());

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
