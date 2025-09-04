use proxmox_schema::{api, ApiStringFormat, IntegerSchema, Schema, StringSchema};
use pve_api_types::{SdnController, SdnVnet, SdnZone};
use serde::{Deserialize, Serialize};

use crate::remotes::REMOTE_ID_SCHEMA;

pub const VXLAN_ID_SCHEMA: Schema = IntegerSchema::new("VXLAN VNI")
    .minimum(1)
    .maximum(16777215)
    .schema();

pub const SDN_ID_SCHEMA: Schema =
    StringSchema::new("The name for an SDN object (zone / vnet / fabric).")
        .format(&ApiStringFormat::VerifyFn(
            pve_api_types::verifiers::verify_sdn_id,
        ))
        .schema();

pub const SDN_CONTROLLER_ID_SCHEMA: Schema = StringSchema::new("The name for an SDN controller.")
    .format(&ApiStringFormat::VerifyFn(
        pve_api_types::verifiers::verify_sdn_controller_id,
    ))
    .schema();

#[api(
    properties: {
        remote: {
            schema: REMOTE_ID_SCHEMA,
        },
        controller: {
            schema: SDN_CONTROLLER_ID_SCHEMA,
        },
    }
)]
/// Describes the remote-specific informations for creating a new zone.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CreateZoneRemote {
    pub remote: String,
    pub controller: String,
}

#[api(
    properties: {
        "vrf-vxlan": {
            schema: VXLAN_ID_SCHEMA,
            optional: true,
        },
        remotes: {
            type: Array,
            description: "List of remotes and the controllers with which the zone should get created.",
            items: {
                type: CreateZoneRemote,
            }
        },
        zone: {
            schema: SDN_ID_SCHEMA,
        },
    }
)]
/// Contains the information for creating a new zone as well as information about the remotes where
/// the zone should get created.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CreateZoneParams {
    pub zone: String,
    pub vrf_vxlan: Option<u32>,
    pub remotes: Vec<CreateZoneRemote>,
}

#[api(
    properties: {
        remote: {
            schema: REMOTE_ID_SCHEMA,
        },
        zone: {
            schema: SDN_ID_SCHEMA,
        },
    }
)]
/// Describes the remote-specific informations for creating a new vnet.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CreateVnetRemote {
    pub remote: String,
    pub zone: String,
}

#[api(
    properties: {
        tag: {
            schema: VXLAN_ID_SCHEMA,
        },
        remotes: {
            type: Array,
            description: "List of remotes and the zones in which the vnet should get created.",
            items: {
                type: CreateVnetRemote,
            }
        },
        vnet: {
            schema: SDN_ID_SCHEMA,
        },
    }
)]
/// Contains the information for creating a new vnet as well as information about the remotes where
/// the vnet should get created.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CreateVnetParams {
    pub tag: u32,
    pub vnet: String,
    pub remotes: Vec<CreateVnetRemote>,
}

#[api(
    properties: {
        remote: {
            schema: REMOTE_ID_SCHEMA,
        },
        controller: {
            type: pve_api_types::SdnController,
            flatten: true,
        }
    }
)]
/// SDN controller with additional information about which remote it belongs to
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct ListController {
    pub remote: String,
    #[serde(flatten)]
    pub controller: SdnController,
}

#[api(
    properties: {
        remote: {
            schema: REMOTE_ID_SCHEMA,
        },
        vnet: {
            type: pve_api_types::SdnVnet,
            flatten: true,
        }
    }
)]
/// SDN controller with additional information about which remote it belongs to
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct ListVnet {
    pub remote: String,
    #[serde(flatten)]
    pub vnet: SdnVnet,
}

#[api(
    properties: {
        remote: {
            schema: REMOTE_ID_SCHEMA,
        },
        zone: {
            type: SdnZone,
            flatten: true,
        }
    }
)]
/// SDN controller with additional information about which remote it belongs to
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct ListZone {
    pub remote: String,
    #[serde(flatten)]
    pub zone: SdnZone,
}
