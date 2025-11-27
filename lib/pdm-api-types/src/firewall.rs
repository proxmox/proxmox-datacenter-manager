use proxmox_schema::{api, Schema};
use serde::{Deserialize, Serialize};

use crate::remotes::REMOTE_ID_SCHEMA;
use crate::{NODE_SCHEMA, VMID_SCHEMA};

const FIREWALL_RULES_COUNT: Schema =
    proxmox_schema::IntegerSchema::new("The total amount of rules present")
        .minimum(0)
        .schema();

const FIREWALL_ACTIVE_RULES_COUNT: Schema =
    proxmox_schema::IntegerSchema::new("The amount of enabled rules")
        .minimum(0)
        .schema();

#[api(
    properties: {
        all: {
            schema: FIREWALL_RULES_COUNT,
        },
        active: {
            schema: FIREWALL_ACTIVE_RULES_COUNT,
        }
    }
)]
/// Count of all rules present and count of all enabled firewall rules.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct RuleStat {
    pub all: usize,
    pub active: usize,
}

#[api(
    properties: {
        enabled: {
            type: bool,
            description: "True if the firewall is enabled",
        },
        rules: {
            type: RuleStat,
            flatten: true,
        },
    }
)]
/// Firewall status.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct FirewallStatus {
    pub enabled: bool,
    #[serde(flatten)]
    pub rules: RuleStat,
}

#[api(
    properties: {
        remote: {
            schema: REMOTE_ID_SCHEMA,
        },
        status: {
            type: FirewallStatus,
            optional: true,
        },
        nodes: {
            description: "Nodes in the cluster",
            items: {
                type: NodeFirewallStatus
            },
            type: Array,
        },
    }
)]
/// Firewall status of a PVE remote.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct RemoteFirewallStatus {
    pub remote: String,
    pub status: Option<FirewallStatus>,
    pub nodes: Vec<NodeFirewallStatus>,
}

#[api(
    properties: {
        node: {
            schema: NODE_SCHEMA,
        },
        status: {
            type: FirewallStatus,
            optional: true,
        },
        guests: {
            description: "Guests on a node",
            items: {
                type: GuestFirewallStatus
            },
            type: Array,
        },
    }
)]
/// Firewall status of a node
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct NodeFirewallStatus {
    pub node: String,
    pub status: Option<FirewallStatus>,
    pub guests: Vec<GuestFirewallStatus>,
}

#[api]
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
/// The type of the guest
pub enum GuestKind {
    /// Guest is a LXC
    Lxc,
    /// Guets is a QEMU
    Qemu,
}

impl GuestKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            GuestKind::Lxc => "lxc",
            GuestKind::Qemu => "qemu",
        }
    }
}

impl AsRef<str> for GuestKind {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<GuestKind> for &'static str {
    fn from(kind: GuestKind) -> Self {
        kind.as_str()
    }
}

impl From<&GuestKind> for &'static str {
    fn from(kind: &GuestKind) -> Self {
        kind.as_str()
    }
}

#[api(
    properties: {
        vmid: {
            schema: VMID_SCHEMA,
        },
        name: {
            type: String,
            description: "Name of the guest.",
        },
        status: {
            type: FirewallStatus,
            optional: true,
        },
        kind: {
            type: GuestKind,
        }
    }
)]
/// Firewall status of a guest
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GuestFirewallStatus {
    pub vmid: u32,
    pub name: String,
    pub status: Option<FirewallStatus>,
    pub kind: GuestKind,
}
