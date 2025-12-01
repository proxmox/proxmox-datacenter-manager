use std::{collections::HashMap, str::FromStr};

use anyhow::Error;
use serde::{Deserialize, Serialize};

use proxmox_schema::api;
use proxmox_subscription::SubscriptionStatus;

#[api]
// order is important here, since we use that for determining if a node has a valid subscription
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
/// Describes the level of subscription
pub enum SubscriptionLevel {
    #[default]
    /// No subscription
    None,
    /// Unknown level of subscription, due to e.g. not being able to fetch it
    Unknown,
    /// Community level subscription
    Community,
    /// Basic level subscription
    Basic,
    /// Standard level subscription
    Standard,
    /// Premium level subscription
    Premium,
}

impl SubscriptionLevel {
    /// Parses the level from a subscription key, such as pve4c-123123123
    pub fn from_key(key: Option<&str>) -> Self {
        match key {
            Some("") | None => SubscriptionLevel::None,
            Some(key) => {
                let (key_type, _) = key.split_once("-").unwrap_or(("", ""));
                if !key_type.is_empty() {
                    Self::from_str(&key_type[key_type.len() - 1..])
                        .ok()
                        .unwrap_or(SubscriptionLevel::Unknown)
                } else {
                    SubscriptionLevel::Unknown
                }
            }
        }
    }
}

impl FromStr for SubscriptionLevel {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "p" => SubscriptionLevel::Premium,
            "s" => SubscriptionLevel::Standard,
            "b" => SubscriptionLevel::Basic,
            "c" => SubscriptionLevel::Community,
            "" => SubscriptionLevel::None,
            _ => SubscriptionLevel::Unknown,
        })
    }
}

impl std::fmt::Display for SubscriptionLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            SubscriptionLevel::None => "",
            SubscriptionLevel::Unknown => "unknown",
            SubscriptionLevel::Community => "c",
            SubscriptionLevel::Basic => "b",
            SubscriptionLevel::Standard => "s",
            SubscriptionLevel::Premium => "p",
        })
    }
}

proxmox_serde::forward_deserialize_to_from_str!(SubscriptionLevel);
proxmox_serde::forward_serialize_to_display!(SubscriptionLevel);

#[api]
#[derive(Default, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// Describes the state of subscription of the remote
pub enum RemoteSubscriptionState {
    #[default]
    /// If there is at least one node with no valid subscription
    None,
    /// If the subscription could not be determined
    Unknown,
    /// If all nodes have subscriptions, but with different levels
    Mixed,
    /// All nodes have the same valid subscription level
    Active,
}

#[api]
#[derive(Default, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// Represents a node subscription information of a remote
pub struct NodeSubscriptionInfo {
    /// The subscription status of the node
    pub status: SubscriptionStatus,

    /// The number of sockets for the node, if relevant for the type of node
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sockets: Option<i64>,

    /// The entered key of the node (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,

    /// The subscription level of the node
    pub level: SubscriptionLevel,

    /// Serverid of the node, if accessible
    #[serde(skip_serializing)]
    pub serverid: Option<String>,
}

#[api(
    properties: {
        "node-status": {
            type: Object,
            optional: true,
            properties: {},
            additional_properties: true,
        },
    },
)]
#[derive(Default, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// Lists the subscription level per node for the remote
pub struct RemoteSubscriptions {
    /// Remote name
    pub remote: String,

    /// Any error that occurred when querying remote resources
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    /// A map from nodename to subscription info
    pub node_status: Option<HashMap<String, Option<NodeSubscriptionInfo>>>,

    pub state: RemoteSubscriptionState,
}

#[api]
#[derive(Default, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// Lists the subscription level per node for the remote
pub struct SubscriptionStatistics {
    /// Total number of nodes across all remotes
    pub total_nodes: usize,
    /// Total number of active subscriptions across all remotes
    pub active_subscriptions: usize,
    /// Total number of community level subscriptions across all remotes
    pub community: usize,
}
