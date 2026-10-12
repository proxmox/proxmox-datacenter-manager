use std::sync::Arc;

use http::StatusCode;
use pdm_api_types::subscription::{ProductType, SubscriptionLevel};
use proxmox_rest_server::{ApiConfig, RestEnvironment};
use proxmox_router::cli::CliEnvironment;
use proxmox_router::{ApiHandler, RpcEnvironment, RpcEnvironmentType, SharedStateRegistry};
use pve_api_types::SetSubscription;

use crate::common::{TestApplication, TestRemoteState, rpcenv};

use server::{
    api,
    context::{ContextFactory, PdmApplication},
};

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
async fn test_reject_stale_digest() {
    common::test_setup();
    let app = TestApplication::new().make_pdm_application().unwrap();
    let (_, old_digest) = app.subscription_key_config().read().unwrap();
    let mut env = rpcenv();

    api::subscriptions::add_keys(
        vec!["pve4c-aaaaaaaaaa".into()],
        Some(old_digest.clone()),
        &mut env,
        app.clone(),
    )
    .await
    .unwrap();

    let (_, current_digest) = app.subscription_key_config().read().unwrap();
    assert_ne!(current_digest, old_digest);
    assert_eq!(env.result_attrib()["digest"], current_digest.to_hex());

    let error = api::subscriptions::add_keys(
        vec!["pve4c-bbbbbbbbbb".into()],
        Some(old_digest),
        &mut rpcenv(),
        app.clone(),
    )
    .await
    .expect_err("stale digest must be rejected");
    assert!(
        error
            .to_string()
            .contains("detected modified configuration")
    );
    assert_eq!(
        app.subscription_key_config().read().unwrap().1,
        current_digest,
    );
    assert_eq!(
        api::subscriptions::list_keys(&mut rpcenv(), app)
            .unwrap()
            .len(),
        1,
    );
}

#[tokio::test]
async fn test_config_lock_excludes_overlapping_transactions() {
    common::test_setup();
    let app = TestApplication::new().make_pdm_application().unwrap();
    let lock = app.subscription_key_config().lock().unwrap();
    assert!(app.subscription_key_config().lock().is_err());
    drop(lock);
    assert!(app.subscription_key_config().lock().is_ok());
}

#[tokio::test]
async fn test_concurrent_additions_preserve_all_keys() {
    common::test_setup();
    let app = TestApplication::new().make_pdm_application().unwrap();

    let results = futures::future::join_all((0..16).map(|i| {
        let app = app.clone();
        async move {
            api::subscriptions::add_keys(vec![format!("pve4c-{i:010}")], None, &mut rpcenv(), app)
                .await
        }
    }))
    .await;

    for result in results {
        assert_eq!(result.unwrap().added, 1);
    }
    assert_eq!(
        api::subscriptions::list_keys(&mut rpcenv(), app)
            .unwrap()
            .len(),
        16,
    );
}

fn rest_environment(app: PdmApplication, kind: RpcEnvironmentType) -> RestEnvironment {
    let mut registry = SharedStateRegistry::default();
    registry.register(app).unwrap();
    let config = ApiConfig::new("/unused", kind).with_shared_state(registry);
    let mut env = RestEnvironment::new(kind, Arc::new(config));
    env.set_auth_id(Some("root@pam".into()));
    env
}

#[tokio::test]
async fn test_handlers_extract_the_registered_application() {
    common::test_setup();
    let app = TestApplication::new().make_pdm_application().unwrap();
    let isolated_app = TestApplication::new().make_pdm_application().unwrap();
    let add = &api::subscriptions::API_METHOD_ADD_KEYS;
    let list = &api::subscriptions::API_METHOD_LIST_KEYS;
    let ApiHandler::Async(add_handler) = add.handler else {
        panic!("expected an asynchronous add handler");
    };
    let ApiHandler::Sync(list_handler) = list.handler else {
        panic!("expected a synchronous list handler");
    };

    let mut public = rest_environment(app.clone(), RpcEnvironmentType::PUBLIC);
    let mut privileged = rest_environment(app.clone(), RpcEnvironmentType::PRIVILEGED);
    let mut isolated = rest_environment(isolated_app, RpcEnvironmentType::PUBLIC);
    add_handler(
        serde_json::json!({"keys": ["pve4c-aaaaaaaaaa"]}),
        add,
        &mut public,
    )
    .await
    .unwrap();

    let entries = list_handler(serde_json::json!({}), list, &mut privileged).unwrap();
    assert_eq!(entries.as_array().unwrap().len(), 1);
    assert!(
        list_handler(serde_json::json!({}), list, &mut isolated)
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );

    let mut cli = CliEnvironment::new();
    cli.set_auth_id(Some("root@pam".into()));
    assert!(list_handler(serde_json::json!({}), list, &mut cli).is_err());
    let mut registry = SharedStateRegistry::default();
    registry.register(app).unwrap();
    cli.set_shared_state_registry(registry);
    assert_eq!(
        list_handler(serde_json::json!({}), list, &mut cli).unwrap(),
        entries,
    );
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
