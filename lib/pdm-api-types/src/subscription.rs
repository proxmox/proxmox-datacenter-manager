use std::sync::OnceLock;
use std::{collections::HashMap, str::FromStr};

use anyhow::Error;
use serde::{Deserialize, Serialize};

use proxmox_config_digest::ConfigDigest;
use proxmox_schema::{ApiStringFormat, ApiType, Schema, StringSchema, api, const_regex};
use proxmox_section_config::typed::ApiSectionDataEntry;
use proxmox_section_config::{SectionConfig, SectionConfigPlugin};
use proxmox_subscription::{SubscriptionInfo, SubscriptionStatus};

use crate::remotes::RemoteType;

#[api]
// order is important here, since we use that for determining if a node has a valid subscription
#[derive(Default, Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
            "p" | "premium" | "Premium" => SubscriptionLevel::Premium,
            "s" | "standard" | "Standard" => SubscriptionLevel::Standard,
            "b" | "basic" | "Basic" => SubscriptionLevel::Basic,
            "c" | "community" | "Community" => SubscriptionLevel::Community,
            "" | "none" | "None" => SubscriptionLevel::None,
            _ => SubscriptionLevel::Unknown,
        })
    }
}

impl std::fmt::Display for SubscriptionLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            SubscriptionLevel::None => "None",
            SubscriptionLevel::Unknown => "Unknown",
            SubscriptionLevel::Community => "Community",
            SubscriptionLevel::Basic => "Basic",
            SubscriptionLevel::Standard => "Standard",
            SubscriptionLevel::Premium => "Premium",
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

    /// Epoch of the last successful subscription check on the node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_time: Option<i64>,

    /// Next due date of the subscription, as reported by the remote.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_due_date: Option<String>,
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

#[api(
    properties: {
        info: {
            type: SubscriptionInfo,
        }
    }
)]
#[derive(Default, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// The PDM subscription info
pub struct PdmSubscriptionInfo {
    #[serde(flatten)]
    pub info: SubscriptionInfo,

    /// PDM subscription statistics
    pub statistics: SubscriptionStatistics,
}

const_regex! {
    /// Subscription key pattern, restricted to the products PDM can drive.
    ///
    /// All keys follow `<prefix>-<10 hex>`. PVE encodes the maximum CPU socket count between
    /// the product letters and the level letter, for example `pve4b-1234567890`. PBS has no
    /// socket count, so its keys look like `pbsc-1234567890`. Level letters are c/b/s/p
    /// (Community/Basic/Standard/Premium).
    ///
    /// PMG and POM keys are not accepted yet: PDM has no remote-side handler for them. Widen
    /// this regex and `ProductType::from_key` in lockstep when PDM grows support for them.
    pub PRODUCT_KEY_REGEX = r"^(?:pve[0-9]+|pbs)[cbsp]-[0-9a-f]{10}$";
}

pub const PRODUCT_KEY_FORMAT: ApiStringFormat = ApiStringFormat::Pattern(&PRODUCT_KEY_REGEX);

pub const SUBSCRIPTION_KEY_SCHEMA: Schema = StringSchema::new("Subscription key.")
    .format(&PRODUCT_KEY_FORMAT)
    .min_length(15)
    .max_length(18)
    .schema();

#[api]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
/// Proxmox product line a subscription key belongs to.
pub enum ProductType {
    /// Proxmox Virtual Environment (PVE).
    #[default]
    Pve,
    /// Proxmox Backup Server (PBS).
    Pbs,
    /// Proxmox Mail Gateway (PMG).
    Pmg,
    /// Proxmox Offline Mirror (POM).
    Pom,
}

impl ProductType {
    /// Static string used as the section-config type marker on disk.
    pub const fn as_section_type(self) -> &'static str {
        match self {
            ProductType::Pve => "pve",
            ProductType::Pbs => "pbs",
            ProductType::Pmg => "pmg",
            ProductType::Pom => "pom",
        }
    }

    /// Classify a key by its prefix.
    ///
    /// Returns None when the prefix does not match any product PDM currently knows about;
    /// callers should log that case so a new product line gets noticed instead of silently
    /// sorted into a default bucket.
    pub fn from_key(key: &str) -> Option<Self> {
        let (prefix, _) = key.split_once('-')?;
        if prefix.starts_with("pve") {
            Some(ProductType::Pve)
        } else if prefix.starts_with("pbs") {
            Some(ProductType::Pbs)
        } else if prefix.starts_with("pmg") {
            Some(ProductType::Pmg)
        } else if prefix.starts_with("pom") {
            Some(ProductType::Pom)
        } else {
            None
        }
    }

    /// Whether PDM currently knows how to drive a remote of this product type.
    ///
    /// PDM only manages PVE and PBS remotes today, and the schema regex rejects everything else
    /// at insert time. This method covers in-memory paths for forward-compat, for example
    /// existing pool entries loaded after the regex is widened in a future release.
    pub fn matches_remote_type(self, remote_type: RemoteType) -> bool {
        matches!(
            (self, remote_type),
            (ProductType::Pve, RemoteType::Pve) | (ProductType::Pbs, RemoteType::Pbs)
        )
    }
}

impl std::fmt::Display for ProductType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_section_type())
    }
}

/// Extract the socket count a PVE key covers (for example, 4 from "pve4b-...").
///
/// Returns None for non-PVE keys or unparseable prefixes.
pub fn socket_count_from_key(key: &str) -> Option<u32> {
    let (prefix, _) = key.split_once('-')?;
    let after_pve = prefix.strip_prefix("pve")?;
    let digits: String = after_pve
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// Pick the candidate PVE key with the smallest socket count that still covers `node_sockets`.
///
/// `candidates` yields `(id, key_string)` pairs. Keys without a parseable PVE socket count are
/// skipped, and keys covering fewer sockets than the node needs are filtered out. Returns the
/// id of the best fit, or None when no candidate covers the node.
pub fn pick_best_pve_socket_key<'a, I, K>(node_sockets: u32, candidates: I) -> Option<K>
where
    I: IntoIterator<Item = (K, &'a str)>,
{
    candidates
        .into_iter()
        .filter_map(|(id, key)| socket_count_from_key(key).map(|s| (id, s)))
        .filter(|(_, s)| *s >= node_sockets)
        .min_by_key(|(_, s)| *s)
        .map(|(id, _)| id)
}

#[api]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
/// Origin of a subscription key entry.
pub enum SubscriptionKeySource {
    /// Hand-entered into the pool by an admin. Used for any key added through the manual-entry
    /// UI or CLI, and as the `serde(default)` for entries that predate this field.
    #[default]
    Manual,
    /// Imported from a remote node's live subscription via the Adopt Key action, that is, a key
    /// that was already installed on a remote before PDM took over its pool management.
    Adopted,
}

#[api(
    properties: {
        "key": { schema: SUBSCRIPTION_KEY_SCHEMA },
        "level": { optional: true },
        "status": { optional: true },
        "source": { optional: true },
        "pending-clear": { optional: true },
    },
)]
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// An entry in the subscription key pool.
pub struct SubscriptionKeyEntry {
    /// The subscription key (for example, pve4b-1234567890).
    pub key: String,

    /// Product type derived from the key prefix.
    #[serde(rename = "product-type")]
    pub product_type: ProductType,

    /// Subscription level, derived from the key suffix.
    #[serde(default)]
    pub level: SubscriptionLevel,

    /// Where the key entry came from. Defaults to manual entry.
    #[serde(default)]
    pub source: SubscriptionKeySource,

    /// Remote this key is assigned to (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,

    /// Node within the remote this key is assigned to (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,

    /// True when the operator queued a clear for this entry's bound node, that is, a request
    /// to free the key from `remote`/`node` so it can be reassigned to a different node.
    ///
    /// Apply Pending issues a DELETE on the remote and then clears `remote`/`node` on success.
    /// Discard Pending only resets this flag and leaves the binding untouched so the operator can
    /// retry. A bare flag is enough since the (remote, node) binding lives next to it.
    ///
    /// Omitted from the serialised representation when false so the on-disk section and the
    /// API response do not carry `pending-clear false` lines for every entry.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pending_clear: bool,

    /// Server ID this key is bound to (from signed info, if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serverid: Option<String>,

    /// Subscription status from last check.
    #[serde(default)]
    pub status: SubscriptionStatus,

    /// Next due date.
    ///
    /// Accepts the upstream `nextduedate` spelling on deserialisation so a future shop-bundle
    /// import path can hand a raw `SubscriptionInfo` blob through without a field-name
    /// translation step; canonical (and on-disk) form is `next-due-date` per the struct's
    /// kebab-case rename.
    #[serde(alias = "nextduedate", skip_serializing_if = "Option::is_none")]
    pub next_due_date: Option<String>,

    /// Product name.
    ///
    /// Accepts the upstream `productname` spelling on deserialisation; canonical form is
    /// `product-name` to stay self-consistent with the sibling `product-type` field.
    #[serde(alias = "productname", skip_serializing_if = "Option::is_none")]
    pub product_name: Option<String>,

    /// Epoch of last import or refresh of this key's data.
    ///
    /// Accepts the upstream `checktime` spelling on deserialisation; canonical form is
    /// `check-time`.
    #[serde(alias = "checktime", skip_serializing_if = "Option::is_none")]
    pub check_time: Option<i64>,
}

impl ApiSectionDataEntry for SubscriptionKeyEntry {
    const INTERNALLY_TAGGED: Option<&'static str> = Some("product-type");
    const SECION_CONFIG_USES_TYPE_KEY: bool = true;

    fn section_config() -> &'static SectionConfig {
        static CONFIG: OnceLock<SectionConfig> = OnceLock::new();

        CONFIG.get_or_init(|| {
            let mut this =
                SectionConfig::new(&SUBSCRIPTION_KEY_SCHEMA).with_type_key("product-type");
            for ty in [
                ProductType::Pve,
                ProductType::Pbs,
                ProductType::Pmg,
                ProductType::Pom,
            ] {
                this.register_plugin(SectionConfigPlugin::new(
                    ty.as_section_type().to_string(),
                    Some("key".to_string()),
                    SubscriptionKeyEntry::API_SCHEMA.unwrap_object_schema(),
                ));
            }
            this
        })
    }

    fn section_type(&self) -> &'static str {
        self.product_type.as_section_type()
    }
}

#[api(
    properties: {
        "key": { schema: SUBSCRIPTION_KEY_SCHEMA },
    },
)]
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
/// Shadow entry storing the signed subscription info blob for a key.
///
/// Currently only populated by the future shop-bundle import flow; manually-added keys leave
/// this table empty. The data layer is in place so that adding the import path later does not
/// require reshaping the on-disk config.
pub struct SubscriptionKeyShadow {
    /// The subscription key.
    pub key: String,

    /// Product type (section type marker).
    #[serde(rename = "product-type")]
    pub product_type: ProductType,

    /// Base64-encoded signed SubscriptionInfo JSON.
    #[serde(default)]
    pub info: String,
}

impl ApiSectionDataEntry for SubscriptionKeyShadow {
    const INTERNALLY_TAGGED: Option<&'static str> = Some("product-type");
    const SECION_CONFIG_USES_TYPE_KEY: bool = true;

    fn section_config() -> &'static SectionConfig {
        static CONFIG: OnceLock<SectionConfig> = OnceLock::new();

        CONFIG.get_or_init(|| {
            let mut this =
                SectionConfig::new(&SUBSCRIPTION_KEY_SCHEMA).with_type_key("product-type");
            for ty in [
                ProductType::Pve,
                ProductType::Pbs,
                ProductType::Pmg,
                ProductType::Pom,
            ] {
                this.register_plugin(SectionConfigPlugin::new(
                    ty.as_section_type().to_string(),
                    Some("key".to_string()),
                    SubscriptionKeyShadow::API_SCHEMA.unwrap_object_schema(),
                ));
            }
            this
        })
    }

    fn section_type(&self) -> &'static str {
        self.product_type.as_section_type()
    }
}

/// Decode a base64-encoded `SubscriptionInfo` JSON blob from the shadow file.
///
/// Forward-compat helper for the future shop-bundle import path. Returns the parsed
/// `SubscriptionInfo`; the caller is responsible for verifying the signature against the shop's
/// signing key.
pub fn parse_signed_info_blob(b64: &str) -> Result<SubscriptionInfo, Error> {
    let bytes = proxmox_base64::decode(b64)?;
    let info = serde_json::from_slice(&bytes)?;
    Ok(info)
}

/// Cross-check the `serverid` of a shadowed entry against what the remote reports.
///
/// Forward-compat helper for the future bundle-import and push flow: when the shadow has a
/// signed serverid binding, the operator should be warned if the remote it is being pushed to
/// has a different hardware id. Returns Ok(None) when there is nothing to compare.
pub fn verify_serverid(
    entry: &SubscriptionKeyEntry,
    remote_info: &SubscriptionInfo,
) -> Result<Option<ServeridMismatch>, Error> {
    let Some(expected) = entry.serverid.as_deref() else {
        return Ok(None);
    };
    let Some(actual) = remote_info.serverid.as_deref() else {
        return Ok(None);
    };
    if expected == actual {
        Ok(None)
    } else {
        Ok(Some(ServeridMismatch {
            key: entry.key.clone(),
            expected: expected.to_string(),
            actual: actual.to_string(),
        }))
    }
}

/// Result of [`verify_serverid`] when the bound and observed server-ids disagree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServeridMismatch {
    pub key: String,
    pub expected: String,
    pub actual: String,
}

#[api]
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
/// Subscription status of a single remote node, combining remote query data with key pool
/// assignment information.
pub struct RemoteNodeStatus {
    /// Remote name.
    pub remote: String,
    /// Remote type (pve or pbs).
    #[serde(rename = "type")]
    pub ty: RemoteType,
    /// Node name.
    pub node: String,
    /// Number of CPU sockets (PVE only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sockets: Option<i64>,
    /// Current subscription status.
    #[serde(default)]
    pub status: SubscriptionStatus,
    /// Subscription level.
    #[serde(default)]
    pub level: SubscriptionLevel,
    /// Currently assigned key from the pool (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_key: Option<String>,
    /// Current key on the node (from remote query).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_key: Option<String>,
    /// True when the pool has a clear queued for this node. Omitted on the wire when false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pending_clear: bool,
    /// Epoch of the last successful subscription check on the node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_time: Option<i64>,
    /// Next due date of the subscription, as reported by the remote.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_due_date: Option<String>,
}

#[api]
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
/// Result of the bulk clear-pending API endpoint.
pub struct ClearPendingResult {
    /// Number of pool entries whose pending push or clear was cleared.
    pub cleared: u32,
}

#[api(
    properties: {
        "key": { schema: SUBSCRIPTION_KEY_SCHEMA },
    },
)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
/// One entry imported by the bulk Adopt-All endpoint.
pub struct AdoptedEntry {
    /// Remote the live subscription was running on.
    pub remote: String,
    /// Node within the remote.
    pub node: String,
    /// The adopted subscription key.
    pub key: String,
}

#[api]
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
/// Result of the add-keys API endpoint.
pub struct AddKeysResult {
    /// Number of keys actually added to the pool.
    pub added: u32,
    /// Number of duplicate keys silently dropped from the input before adding.
    pub deduplicated: u32,
}

#[api]
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
/// A proposed key-to-node assignment from the auto-assign algorithm.
pub struct ProposedAssignment {
    /// The subscription key to assign.
    pub key: String,
    /// Target remote.
    pub remote: String,
    /// Target node.
    pub node: String,
    /// Socket count of the key (PVE only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_sockets: Option<u32>,
    /// Socket count of the node (PVE only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_sockets: Option<i64>,
}

#[api(
    properties: {
        assignments: {
            type: Array,
            description: "Proposed assignments. Empty when nothing matches.",
            items: { type: ProposedAssignment },
        },
        "keys-digest": { type: ConfigDigest },
    },
)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
/// The full plan returned by auto-assign and accepted by bulk-assign.
///
/// `keys_digest` and `node_status_digest` are snapshots taken when the plan was computed.
/// `bulk_assign` rejects the plan with 409 if either has changed in the meantime, so the
/// operator never silently commits a plan that no longer matches the live state.
pub struct AutoAssignProposal {
    /// Proposed assignments. Empty when nothing matches.
    pub assignments: Vec<ProposedAssignment>,
    /// Digest of the key pool config the proposal was computed against.
    pub keys_digest: ConfigDigest,
    /// SHA-256 over the relevant slice of node status (sorted JSON) at proposal time.
    pub node_status_digest: String,
}
