use pdm_api_types::firewall::{FirewallStatus, GuestFirewallStatus, GuestKind, RuleStat};
use pwt::props::ExtractPrimaryKey;
use std::fmt;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Scope {
    /// Show all remotes, nodes, and guests
    All,
    /// Show specific remote with all its nodes and guests
    Remote { name: String },
    /// Show specific node with all its guests
    Node { remote: String, name: String },
}

impl Default for Scope {
    fn default() -> Self {
        Self::All
    }
}

impl Scope {
    pub fn remote_name(&self) -> Option<&str> {
        match self {
            Self::All => None,
            Self::Remote { name } | Self::Node { remote: name, .. } => Some(name),
        }
    }

    pub fn node_name(&self) -> Option<&str> {
        match self {
            Self::Node { name, .. } => Some(name),
            _ => None,
        }
    }

    pub fn loading_description(&self) -> String {
        match self {
            Self::All => "all remotes".to_string(),
            Self::Remote { name } => format!("remote {}", name),
            Self::Node { remote, name } => format!("node {}/{}", remote, name),
        }
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::All => write!(f, "All"),
            Self::Remote { name } => write!(f, "{}", name),
            Self::Node { remote, name } => write!(f, "{}/{}", remote, name),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoadState {
    pub data_generation: usize,
    pub nodes_generation: usize,
    pub is_loading: bool,
}

impl Default for LoadState {
    fn default() -> Self {
        Self {
            data_generation: 0,
            nodes_generation: 0,
            is_loading: false,
        }
    }
}

impl LoadState {
    pub fn start_data_load(&mut self) -> usize {
        self.data_generation = self.data_generation.wrapping_add(1);
        self.is_loading = true;
        self.data_generation
    }

    pub fn start_nodes_load(&mut self) -> usize {
        self.nodes_generation = self.nodes_generation.wrapping_add(1);
        self.nodes_generation
    }

    pub fn finish_load(&mut self) {
        self.is_loading = false;
    }

    pub fn is_data_current(&self, generation: usize) -> bool {
        generation == self.data_generation
    }

    pub fn is_nodes_current(&self, generation: usize) -> bool {
        generation == self.nodes_generation
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct GuestEntry {
    pub guest: GuestFirewallStatus,
    pub node: String,
    pub remote: String,
    pub masked: bool,
}

impl GuestEntry {
    pub fn new(guest: GuestFirewallStatus, node: String, remote: String, masked: bool) -> Self {
        Self {
            guest,
            node,
            remote,
            masked,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum TreeEntry {
    Root,
    Remote(RemoteEntry),
    Node(NodeEntry),
    Guest(GuestEntry, GuestKind),
}

#[derive(Clone, PartialEq, Debug)]
pub struct RemoteEntry {
    pub name: String,
    pub status: Option<FirewallStatus>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct NodeEntry {
    pub remote: String,
    pub name: String,
    pub status: Option<FirewallStatus>,
    pub masked: bool,
}

impl TreeEntry {
    pub fn name(&self) -> String {
        match self {
            Self::Root => String::new(),
            Self::Remote(entry) => entry.name.clone(),
            Self::Node(entry) => entry.name.clone(),
            Self::Guest(guest, _) => {
                format!("{} ({})", guest.guest.vmid, guest.guest.name)
            }
        }
    }

    pub fn matches_filter(&self, filter_text: &str) -> bool {
        match self {
            Self::Root | Self::Remote(..) | Self::Node(..) => true,
            Self::Guest(guest, kind) => {
                let type_name = kind.as_str();
                guest.guest.name.to_lowercase().contains(filter_text)
                    || guest.guest.vmid.to_string().contains(filter_text)
                    || type_name.contains(filter_text)
                    || guest.node.to_lowercase().contains(filter_text)
                    || guest.remote.to_lowercase().contains(filter_text)
            }
        }
    }

    pub fn icon_name(&self) -> Option<&'static str> {
        match self {
            Self::Remote(..) => Some("server"),
            Self::Node(..) => Some("building"),
            Self::Guest(_, GuestKind::Lxc) => Some("cube"),
            Self::Guest(_, GuestKind::Qemu) => Some("desktop"),
            Self::Root => None,
        }
    }

    pub fn firewall_status(&self) -> Option<(&FirewallStatus, bool)> {
        match self {
            Self::Remote(entry) => entry.status.as_ref().map(|s| (s, false)),
            Self::Node(entry) => entry.status.as_ref().map(|s| (s, entry.masked)),
            Self::Guest(guest, _) => guest.guest.status.as_ref().map(|s| (s, guest.masked)),
            Self::Root => None,
        }
    }

    pub fn rule_stats(&self) -> Option<&RuleStat> {
        self.firewall_status().map(|(status, _)| &status.rules)
    }

    pub fn sort_rank(&self) -> u8 {
        match self {
            Self::Root => 0,
            Self::Remote(..) => 1,
            Self::Node(..) => 2,
            Self::Guest(_, GuestKind::Lxc) => 3,
            Self::Guest(_, GuestKind::Qemu) => 4,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Remote(..) => "remote",
            Self::Node(..) => "node",
            Self::Guest(_, GuestKind::Lxc) => "lxc",
            Self::Guest(_, GuestKind::Qemu) => "qemu",
        }
    }
}

impl ExtractPrimaryKey for TreeEntry {
    fn extract_key(&self) -> yew::virtual_dom::Key {
        use yew::virtual_dom::Key;
        match self {
            Self::Root => Key::from("root"),
            Self::Remote(entry) => Key::from(format!("remote-{}", entry.name)),
            Self::Node(entry) => Key::from(format!("{}/{}", entry.remote, entry.name)),
            Self::Guest(guest, _) => Key::from(format!(
                "{}/{}/{}",
                guest.remote, guest.node, guest.guest.vmid
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub enum FirewallError {
    RemoteListLoadFailed(String),
    StatusLoadFailed { scope: Scope, message: String },
    NodesLoadFailed { remote: String, message: String },
}

impl fmt::Display for FirewallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RemoteListLoadFailed(msg) => {
                write!(f, "Failed to load remote list: {}", msg)
            }
            Self::StatusLoadFailed { scope, message } => {
                write!(
                    f,
                    "Failed to load firewall status for {}: {}",
                    scope, message
                )
            }
            Self::NodesLoadFailed { remote, message } => {
                write!(f, "Failed to load nodes for remote {}: {}", remote, message)
            }
        }
    }
}

impl std::error::Error for FirewallError {}
