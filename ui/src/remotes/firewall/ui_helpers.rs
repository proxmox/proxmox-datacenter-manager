use pdm_api_types::firewall::{FirewallStatus, GuestKind, RuleStat};
use pwt::css::{AlignItems, FontColor};
use pwt::prelude::*;
use pwt::tr;
use pwt::widget::{Container, Fa, Row};
use yew::{html, Html};

use proxmox_yew_comp::configuration::pve::FirewallRulesPanel;
use proxmox_yew_comp::form::pve::PveGuestType;

use super::types::TreeEntry;

pub fn render_firewall_status(status: &FirewallStatus, masked: bool) -> Html {
    if status.enabled {
        let check = if !masked {
            Fa::new("check").class(FontColor::Success)
        } else {
            Fa::new("check")
        };
        Row::new()
            .class(AlignItems::Baseline)
            .gap(2)
            .with_child(check)
            .into()
    } else {
        Row::new()
            .class(AlignItems::Baseline)
            .gap(2)
            .with_child(Fa::new("minus"))
            .into()
    }
}

pub fn render_rule_stats(rules: &RuleStat) -> Html {
    if rules.all == 0 {
        return Html::default();
    }
    Row::new()
        .with_child(format!("{} of {} rules enabled", rules.active, rules.all))
        .into()
}

pub fn render_warning_icon() -> Html {
    Row::new()
        .with_child(Fa::new("exclamation-triangle").class(FontColor::Warning))
        .into()
}

pub fn render_load_error_message() -> Html {
    Row::new().with_child(tr!("Could not load status")).into()
}

pub fn create_panel_title(icon_name: &str, title_text: String) -> Html {
    Row::new()
        .gap(2)
        .class(AlignItems::Baseline)
        .with_child(Fa::new(icon_name))
        .with_child(title_text)
        .into()
}

pub struct PanelConfig {
    pub title: Html,
    pub content: Html,
}

impl PanelConfig {
    pub fn for_remote(remote: &str) -> Self {
        let rules = FirewallRulesPanel::cluster_firewall()
            .remote(remote.to_string())
            .readonly(true);
        Self {
            title: create_panel_title("server", tr!("Cluster Firewall - {}", remote)),
            content: rules.into(),
        }
    }

    pub fn for_node(remote: &str, node: &str) -> Self {
        let rules = FirewallRulesPanel::node_firewall(node.to_string())
            .remote(remote.to_string())
            .readonly(true);
        Self {
            title: create_panel_title("building", tr!("Node Firewall - {0}/{1}", remote, node)),
            content: rules.into(),
        }
    }

    pub fn for_guest(remote: &str, node: &str, vmid: u32, guest_type: PveGuestType) -> Self {
        let rules = FirewallRulesPanel::guest_firewall(guest_type, node.to_string(), vmid)
            .remote(remote.to_string())
            .readonly(true);
        let (prefix, icon) = match guest_type {
            PveGuestType::Lxc => ("CT", "cube"),
            PveGuestType::Qemu => ("VM", "desktop"),
        };
        Self {
            title: create_panel_title(
                icon,
                tr!(
                    "Guest Firewall - {0}/{1}/{2} {3}",
                    remote,
                    node,
                    prefix,
                    vmid
                ),
            ),
            content: rules.into(),
        }
    }

    pub fn for_no_selection() -> Self {
        let header = tr!("No entry selected");
        let msg = tr!("Select a firewall entry to show its rules.");

        let content = pwt::widget::Column::new()
            .class(pwt::css::FlexFit)
            .padding(2)
            .class(AlignItems::Center)
            .class(pwt::css::TextAlign::Center)
            .with_child(html! {<h1 class="pwt-font-headline-medium">{header}</h1>})
            .with_child(Container::new().with_child(msg))
            .into();

        Self {
            title: create_panel_title("shield", tr!("Firewall")),
            content,
        }
    }

    pub fn from_entry(entry: &TreeEntry) -> Self {
        match entry {
            TreeEntry::Remote(remote_entry) => Self::for_remote(&remote_entry.name),
            TreeEntry::Node(node_entry) => Self::for_node(&node_entry.remote, &node_entry.name),
            TreeEntry::Guest(guest, kind) => {
                let guest_type = match *kind {
                    GuestKind::Lxc => PveGuestType::Lxc,
                    GuestKind::Qemu => PveGuestType::Qemu,
                };
                Self::for_guest(&guest.remote, &guest.node, guest.guest.vmid, guest_type)
            }
            TreeEntry::Root => Self::for_no_selection(),
        }
    }
}
