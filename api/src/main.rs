use std::future::Future;
use std::pin::Pin;

use anyhow::Error;
use http::request::Parts;
use http::HeaderMap;
use hyper::{Body, Method, Response};
use serde_json::{json, Value};

use proxmox_router::{
    list_subdirs_api_method, Router, RpcEnvironmentType, SubdirMap, UserInformation,
};
use proxmox_schema::api;
use proxmox_rest_server::{ApiConfig, AuthError, RestEnvironment, RestServer, ServerAdapter};

use pdm_buildcfg;

// Create a Dummy User information system
struct DummyUserInfo;

impl UserInformation for DummyUserInfo {
    fn is_superuser(&self, _userid: &str) -> bool {
        // Always return true here, so we have access to everything
        true
    }
    fn is_group_member(&self, _userid: &str, group: &str) -> bool {
        group == "Group"
    }
    fn lookup_privs(&self, _userid: &str, _path: &[&str]) -> u64 {
        u64::MAX
    }
}

struct MinimalServer;

// implement the server adapter
impl ServerAdapter for MinimalServer {
    // normally this would check and authenticate the user
    fn check_auth(
        &self,
        _headers: &HeaderMap,
        _method: &Method,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<(String, Box<dyn UserInformation + Sync + Send>), AuthError>>
                + Send,
        >,
    > {
        Box::pin(async move {
            // get some global/cached userinfo
            let userinfo: Box<dyn UserInformation + Sync + Send> = Box::new(DummyUserInfo);
            // Do some user checks, e.g. cookie/csrf
            Ok(("User".to_string(), userinfo))
        })
    }

    // this should return the index page of the webserver, iow. what the user browses to
    fn get_index(
        &self,
        _env: RestEnvironment,
        _parts: Parts,
    ) -> Pin<Box<dyn Future<Output = Response<Body>> + Send>> {
        Box::pin(async move {
            // build an index page
            http::Response::builder()
                .body("hello world".into())
                .unwrap()
        })
    }
}

#[api]
/// A simple ping method. returns "pong"
fn ping() -> Result<String, Error> {
    Ok("pong".to_string())
}

#[api]
/// Return the program's version/release info
fn version() -> Result<Value, Error> {
    Ok(json!({
        "version": pdm_buildcfg::PROXMOX_PKG_VERSION,
        "release": pdm_buildcfg::PROXMOX_PKG_RELEASE,
        "repoid": pdm_buildcfg::PROXMOX_PKG_REPOID
    }))
}

// NOTE: must be sorted!
const SUBDIRS: SubdirMap = &[
    ("ping", &Router::new().get(&API_METHOD_PING)),
    ("version", &Router::new().get(&API_METHOD_VERSION)),
];

const ROUTER: Router = Router::new()
    .get(&list_subdirs_api_method!(SUBDIRS))
    .subdirs(SUBDIRS);

async fn run() -> Result<(), Error> {
    let config = ApiConfig::new(
        "/var/tmp/",
        &ROUTER,
        RpcEnvironmentType::PUBLIC,
        MinimalServer,
    )?;
    let rest_server = RestServer::new(config);

    proxmox_rest_server::daemon::create_daemon(
        ([127, 0, 0, 1], 65000).into(),
        move |listener| {
            let incoming = hyper::server::conn::AddrIncoming::from_listener(listener)?;

            Ok(async move {
                hyper::Server::builder(incoming).serve(rest_server).await?;

                Ok(())
            })
        },
        None,
    )
    .await?;

    Ok(())
}

fn main() -> Result<(), Error> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async { run().await })
}
