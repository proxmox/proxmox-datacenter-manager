use proxmox_schema::{api, const_regex, ApiStringFormat, IntegerSchema, Schema, StringSchema};
use pve_api_types::SdnZone;
use serde::{Deserialize, Serialize};

use crate::remotes::REMOTE_ID_SCHEMA;

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
