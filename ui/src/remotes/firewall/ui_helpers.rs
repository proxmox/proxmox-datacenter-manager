use pdm_api_types::firewall::{FirewallStatus, GuestKind, RuleStat};
use pwt::css::{AlignItems, FontColor};
use pwt::prelude::*;
use pwt::tr;
use pwt::widget::{Container, Fa, Panel, Row};
use yew::{html, Html};

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

pub fn create_rules_panel(title: Html, key: String, content: Html) -> Panel {
    Panel::new()
        .class(pwt::css::FlexFit)
        .title(title)
        .border(true)
        .min_width(500)
        .with_child(Container::new().key(key).with_child(content))
        .style("flex", "1 1 0")
}

pub struct PanelConfig {
    pub title: Html,
    pub key: String,
    pub content: Html,
    pub title_prefix: Option<Html>,
}

impl PanelConfig {
    pub fn for_remote(remote: &str, reload_token: usize) -> Self {
        let mut rules = proxmox_yew_comp::FirewallRules::cluster(remote.to_string());
        rules.reload_token = reload_token;
        Self {
            title: create_panel_title("list", tr!("Cluster Firewall Rules - {}", remote)),
            key: format!("cluster-{}", remote),
            content: rules.into(),
            title_prefix: None,
        }
    }

    pub fn for_node(remote: &str, node: &str, reload_token: usize) -> Self {
        let mut rules = proxmox_yew_comp::FirewallRules::node(remote.to_string(), node.to_string());
        rules.reload_token = reload_token;
        Self {
            title: create_panel_title("list", tr!("Node Firewall Rules - {0}/{1}", remote, node)),
            key: format!("node-{}-{}", remote, node),
            content: rules.into(),
            title_prefix: None,
        }
    }

    pub fn for_guest(
        remote: &str,
        node: &str,
        vmid: u32,
        kind: GuestKind,
        reload_token: usize,
    ) -> Self {
        let vmtype = kind.as_str();
        let mut rules = proxmox_yew_comp::FirewallRules::guest(
            remote.to_string(),
            node.to_string(),
            vmid as u64,
            vmtype,
        );
        rules.reload_token = reload_token;
        Self {
            title: create_panel_title(
                "list",
                tr!(
                    "Guest Firewall Rules - {0}/{1}/{2} {3}",
                    remote,
                    node,
                    vmtype.to_uppercase(),
                    vmid
                ),
            ),
            key: format!("guest-{}-{}-{}-{}", remote, node, vmid, vmtype),
            content: rules.into(),
            title_prefix: None,
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
            title: create_panel_title("list", tr!("Firewall Rules")),
            key: String::new(),
            content,
            title_prefix: None,
        }
    }

    pub fn from_entry(entry: &TreeEntry, reload_token: usize) -> Self {
        match entry {
            TreeEntry::Remote(remote_entry) => Self::for_remote(&remote_entry.name, reload_token),
            TreeEntry::Node(node_entry) => {
                Self::for_node(&node_entry.remote, &node_entry.name, reload_token)
            }
            TreeEntry::Guest(guest, kind) => Self::for_guest(
                &guest.remote,
                &guest.node,
                guest.guest.vmid,
                *kind,
                reload_token,
            ),
            TreeEntry::Root => Self::for_no_selection(),
        }
    }

    pub fn build(self) -> Panel {
        let title = if let Some(prefix) = self.title_prefix {
            Row::new()
                .gap(2)
                .class(AlignItems::Baseline)
                .with_child(prefix)
                .with_child(self.title)
                .into()
        } else {
            self.title
        };
        create_rules_panel(title, self.key, self.content)
    }
}
