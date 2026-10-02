use http::StatusCode;
use pdm_api_types::subscription::{ProductType, SubscriptionLevel};
use pve_api_types::SetSubscription;

use crate::common::{TestApplication, TestRemoteState, rpcenv};

use server::{api, context::ContextFactory};

pub mod common;

#[tokio::test]
async fn test_manage_inventory() {
    common::test_setup();

    let test_app = TestApplication::new();
    let app = test_app.make_pdm_application().unwrap();

    let existing_keys = api::subscriptions::list_keys(&mut rpcenv(), app.clone()).unwrap();
    assert!(existing_keys.is_empty());

    let keys = vec![
        "pve4c-aaaaaaaaaa".into(),
        "pve4c-bbbbbbbbbb".into(),
        "pve4c-bbbbbbbbbb".into(),
    ];

    let add_result = api::subscriptions::add_keys(keys, None, &mut rpcenv(), app.clone())
        .await
        .unwrap();

    assert_eq!(add_result.added, 2);
    assert_eq!(add_result.deduplicated, 1);

    let existing_keys = api::subscriptions::list_keys(&mut rpcenv(), app.clone()).unwrap();
    assert_eq!(existing_keys.len(), 2);

    let key =
        api::subscriptions::get_key("pve4c-aaaaaaaaaa".into(), &mut rpcenv(), app.clone()).unwrap();

    assert_eq!(key.key, "pve4c-aaaaaaaaaa");
    assert_eq!(key.product_type, ProductType::Pve);
    assert_eq!(key.level, SubscriptionLevel::Community);
    assert_eq!(key.remote, None);

    api::subscriptions::delete_key("pve4c-aaaaaaaaaa".into(), None, &mut rpcenv(), app.clone())
        .await
        .unwrap();

    let result = api::subscriptions::get_key("pve4c-aaaaaaaaaa".into(), &mut rpcenv(), app.clone());

    common::assert_http_error!(result, StatusCode::NOT_FOUND, "not found in pool");
}

#[tokio::test]
async fn test_invalid_key_format() {
    common::test_setup();

    let test_app = TestApplication::new();
    let app = test_app.make_pdm_application().unwrap();

    let keys = vec!["aaaaa".into()];

    let result = api::subscriptions::add_keys(keys, None, &mut rpcenv(), app.clone()).await;

    common::assert_http_error!(result, StatusCode::BAD_REQUEST, "unrecognised key format");
}

common::test_pve_client!(SubscriptionClient {
    async fn set_subscription(&self, node: &str, params: SetSubscription) -> Result<(), proxmox_client::Error> {
        self.state().set(format!("set_subscription-{node}"), &params);

        Ok(())
    }
});

#[tokio::test]
async fn test_apply_key() {
    common::test_setup();

    let remote_state = TestRemoteState::default();

    let test_app = TestApplication::new().with_pve_remote(
        "remote-a",
        remote_state.clone(),
        SubscriptionClient,
    );

    let app = test_app.make_pdm_application().unwrap();

    let keys = vec![
        "pve4c-aaaaaaaaaa".into(),
        "pve4c-bbbbbbbbbb".into(),
        "pve4c-bbbbbbbbbb".into(),
    ];
    let _ = api::subscriptions::add_keys(keys.clone(), None, &mut rpcenv(), app.clone())
        .await
        .unwrap();

    api::subscriptions::set_assignment(
        keys[0].clone(),
        "remote-a".into(),
        "remote-a-node-0".into(),
        None,
        &mut rpcenv(),
        app.clone(),
    )
    .await
    .unwrap();

    let upid = api::subscriptions::apply_pending(None, &mut rpcenv(), app.clone())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(common::wait_for_task(&upid).await.unwrap(), "OK");

    let set_subscription: SetSubscription = remote_state
        .get("set_subscription-remote-a-node-0")
        .expect("subscription was set on remote-a-node-0");

    assert_eq!(set_subscription.key, keys[0]);
}
