use anyhow::{anyhow, Error};
use http::StatusCode;

use pdm_api_types::{remotes::REMOTE_ID_SCHEMA, sdn::SDN_ID_SCHEMA, NODE_SCHEMA};
use proxmox_router::{list_subdirs_api_method, Router, SubdirMap};
use proxmox_schema::api;
use pve_api_types::SdnZoneIpVrf;

use crate::api::pve::{connect, get_remote};

mod zones {
    use super::*;

    const ZONE_SUBDIRS: SubdirMap = &[("ip-vrf", &Router::new().get(&API_METHOD_GET_IP_VRF))];

    const ZONE_ROUTER: Router = Router::new()
        .get(&list_subdirs_api_method!(ZONE_SUBDIRS))
        .subdirs(ZONE_SUBDIRS);

    pub const ROUTER: Router = Router::new().match_all("zone", &ZONE_ROUTER);

    #[api(
        input: {
            properties: {
                remote: { schema: REMOTE_ID_SCHEMA },
                node: { schema: NODE_SCHEMA },
                zone: { schema: SDN_ID_SCHEMA },
            },
        },
        returns: { type: SdnZoneIpVrf },
    )]
    /// Get the IP-VRF for an EVPN zone for a node on a given remote
    async fn get_ip_vrf(
        remote: String,
        node: String,
        zone: String,
    ) -> Result<Vec<SdnZoneIpVrf>, Error> {
        let (remote_config, _) = pdm_config::remotes::config()?;
        let remote = get_remote(&remote_config, &remote)?;
        let client = connect(remote)?;

        client
            .get_zone_ip_vrf(&node, &zone)
            .await
            .map_err(|err| match err {
                proxmox_client::Error::Api(StatusCode::NOT_IMPLEMENTED, _msg) => {
                    anyhow!("remote {} does not support the zone ip-vrf API call, please upgrade to the newest version!", remote.id)
                }
                _ => err.into()
            })
    }
}

const SUBDIRS: SubdirMap = &[("zone", &zones::ROUTER)];

pub const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);
