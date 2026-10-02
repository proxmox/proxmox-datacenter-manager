use pve_api_types::{ListZonesType, SdnVnet, SdnVnetType, SdnZone};
use server::context::ContextFactory;

use crate::common::{TestApplication, rpcenv};

pub mod common;

common::test_pve_client!(SdnPveClient {
    async fn list_zones(
        &self,
        _pending: Option<bool>,
        _running: Option<bool>,
        _ty: Option<ListZonesType>,
    ) -> Result<Vec<SdnZone>, proxmox_client::Error> {
        common::read_captured_response(&format!(
            "tests/api_responses/pve/{}/list_zones.json",
            self.remote(),
        )).await
    }

    async fn list_vnets(
        &self,
        _pending: Option<bool>,
        _running: Option<bool>,
    ) -> Result<Vec<SdnVnet>, proxmox_client::Error> {
        common::read_captured_response(&format!(
            "tests/api_responses/pve/{}/list_vnets.json",
            self.remote(),
        )).await
    }
});

#[tokio::test]
async fn test_lists_zones() {
    common::test_setup();

    let test_app = TestApplication::new()
        .with_pve_remote("remote-a", Default::default(), SdnPveClient)
        .with_pve_remote("remote-b", Default::default(), SdnPveClient);

    let app = test_app.make_pdm_application().unwrap();

    let mut result =
        server::api::sdn::zones::list_zones(None, None, None, None, &mut rpcenv(), app)
            .await
            .unwrap();

    // We don't guarantee any ordering
    result.sort_by(|a, b| a.remote.cmp(&b.remote));

    assert_eq!(result.len(), 2);

    assert_eq!(result[0].remote, "remote-a");
    assert_eq!(result[0].zone.zone, "aaaa");
    assert_eq!(result[0].zone.ty, ListZonesType::Simple);
    assert_eq!(result[1].remote, "remote-b");
    assert_eq!(result[1].zone.zone, "bbbb");
    assert_eq!(result[1].zone.ty, ListZonesType::Simple);
}

#[tokio::test]
async fn test_lists_vnets() {
    common::test_setup();

    let test_app = TestApplication::new()
        .with_pve_remote("remote-a", Default::default(), SdnPveClient)
        .with_pve_remote("remote-b", Default::default(), SdnPveClient);

    let app = test_app.make_pdm_application().unwrap();

    let mut result = server::api::sdn::vnets::list_vnets(None, None, None, &mut rpcenv(), app)
        .await
        .unwrap();

    // We don't guarantee any ordering
    result.sort_by(|a, b| a.remote.cmp(&b.remote));

    assert_eq!(result.len(), 2);

    assert_eq!(result[0].remote, "remote-a");
    assert_eq!(result[0].vnet.vnet, "aaaavnet");
    assert_eq!(result[0].vnet.ty, SdnVnetType::Vnet);
    assert_eq!(result[1].remote, "remote-b");
    assert_eq!(result[1].vnet.vnet, "bbbbvnet");
    assert_eq!(result[1].vnet.ty, SdnVnetType::Vnet);
}
