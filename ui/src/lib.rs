use js_sys::{Array, JsString, Object};
use pdm_api_types::remote_updates::RemoteUpdateSummary;
use pdm_api_types::remotes::RemoteType;
use pdm_api_types::resource::{PveLxcResource, PveQemuResource};
use pdm_client::types::Resource;
use proxmox_deb_version::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use proxmox_yew_comp::http_get;

mod administration;

pub use administration::ServerAdministration;

mod certificates;
pub use certificates::CertificatesPanel;

mod configuration;
pub use configuration::{AccessControl, SystemConfiguration};

mod main_menu;
pub use main_menu::MainMenu;

mod remotes;
pub use remotes::RemoteConfigPanel;

mod top_nav_bar;
pub use top_nav_bar::TopNavBar;

mod search_provider;
pub use search_provider::SearchProvider;

mod dashboard;

use yew_router::prelude::RouterScopeExt;

mod widget;

pub mod pbs;
pub mod pve;

pub mod sdn;

pub mod renderer;

mod load_result;
pub use load_result::LoadResult;

mod tasks;
pub use tasks::register_pve_tasks;

mod view_list_context;
pub use view_list_context::ViewListContext;

pub fn pdm_client() -> pdm_client::PdmClient<std::rc::Rc<proxmox_yew_comp::HttpClientWasm>> {
    pdm_client::PdmClient(proxmox_yew_comp::CLIENT.with(|c| std::rc::Rc::clone(&c.borrow())))
}

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RemoteList(pub Vec<pdm_client::types::Remote>);

impl From<Vec<pdm_client::types::Remote>> for RemoteList {
    fn from(value: Vec<pdm_client::types::Remote>) -> Self {
        Self(value)
    }
}

impl std::ops::Deref for RemoteList {
    type Target = [pdm_client::types::Remote];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteListCacheEntry {
    pub ty: RemoteType,
    pub id: String,
}

/// Get the global remote list if loaded
pub(crate) fn get_remote_list<C: yew::Component>(link: &yew::html::Scope<C>) -> Option<RemoteList> {
    let (list, _) = link.context(yew::Callback::from(|_: RemoteList| {}))?;
    Some(list)
}

/// Get the given remote from the global remote list if loaded
pub(crate) fn get_remote<C: yew::Component>(
    link: &yew::html::Scope<C>,
    id: &str,
) -> Option<pdm_client::types::Remote> {
    for remote in get_remote_list(link)?.iter() {
        if remote.id == id {
            return Some(remote.clone());
        }
    }

    None
}

/// Get a deep link to the given remote/id pair
///
/// Returns None if the remote can't be found, or there is no global remote list
pub(crate) fn get_deep_url<C: yew::Component>(
    link: &yew::html::Scope<C>,
    remote: &str,
    _node: Option<&str>,
    id: &str,
) -> Option<web_sys::Url> {
    let hash = match (id, get_remote(link, remote)?.ty) {
        ("", _) => String::new(),
        (id, pdm_api_types::remotes::RemoteType::Pve) => format!("v1::={id}"),
        (id, pdm_api_types::remotes::RemoteType::Pbs) => format!("DataStore-{id}"),
    };
    get_deep_url_low_level(link, remote, _node, &hash)
}

/// Get a deep link to the given remote/low-level-hash pair
///
/// The hash is the lower level route. It not only specific to a product but also has a hash format
/// version there and depending on the product version not all components might be supported.
/// While the format version itself was not yet bumped as of PVE 9, new entries get added
/// frequently.
///
/// Returns None if the remote can't be found, or there is no global remote list
pub(crate) fn get_deep_url_low_level<C: yew::Component>(
    link: &yew::html::Scope<C>,
    remote: &str,
    _node: Option<&str>,
    hash: &str,
) -> Option<web_sys::Url> {
    let remote = get_remote(link, remote)?;
    let url = remote
        .web_url
        .and_then(|orig_url| {
            let mut parts = orig_url.clone().into_parts();
            if parts.scheme.is_none() {
                parts.scheme = Some(http::uri::Scheme::HTTPS);
                if parts.path_and_query.is_none() {
                    parts.path_and_query = Some(http::uri::PathAndQuery::from_static("/"));
                }
            }
            http::Uri::from_parts(parts)
                .inspect_err(|err| {
                    log::error!(
                        "failed to rebuild URL from {orig_url:?} with scheme 'https' - {err:?}"
                    )
                })
                .ok()
        })
        .and_then(|url| web_sys::Url::new(&url.to_string()).ok())
        .or_else(|| {
            let node = remote.nodes.first()?;
            let url = web_sys::Url::new(&format!("https://{}/", node.hostname));
            url.ok().inspect(|url| {
                if url.port() == "" {
                    let default_port = match remote.ty {
                        pdm_api_types::remotes::RemoteType::Pve => "8006",
                        pdm_api_types::remotes::RemoteType::Pbs => "8007",
                    };
                    url.set_port(default_port);
                }
            })
        });

    url.inspect(|url| {
        url.set_hash(hash);
    })
}

pub(crate) fn navigate_to<C: yew::Component>(
    link: &yew::html::Scope<C>,
    remote: &str,
    resource: Option<&pdm_client::types::Resource>,
) {
    if let Some(nav) = link.navigator() {
        let (prefix, id) = resource
            .and_then(|resource| {
                Some(match resource {
                    pdm_client::types::Resource::PveQemu(PveQemuResource { vmid, .. })
                    | pdm_client::types::Resource::PveLxc(PveLxcResource { vmid, .. }) => {
                        (true, format!("guest+{vmid}"))
                    }
                    pdm_client::types::Resource::PveNode(node) => {
                        (true, format!("node+{}", node.node))
                    }
                    pdm_client::types::Resource::PveStorage(storage) => (
                        true,
                        format!("storage+{}+{}", storage.node, storage.storage),
                    ),
                    pdm_client::types::Resource::PveNetwork(_) => (false, "sdn".to_string()),
                    pdm_client::types::Resource::PbsDatastore(store) => (true, store.name.clone()),
                    // FIXME: implement
                    _ => return None,
                })
            })
            .unwrap_or_else(|| (true, String::new()));

        let prefix = if prefix {
            format!("remote-{remote}/")
        } else {
            String::new()
        };

        nav.push(&yew_router::AnyRoute::new(format!("/{prefix}{id}")));
    }
}

pub(crate) fn get_resource_node(resource: &Resource) -> Option<&str> {
    match resource {
        Resource::PveStorage(storage) => Some(&storage.node),
        Resource::PveQemu(qemu) => Some(&qemu.node),
        Resource::PveLxc(lxc) => Some(&lxc.node),
        Resource::PveNode(node) => Some(&node.node),
        Resource::PveNetwork(network) => Some(network.node()),
        Resource::PbsNode(_) => None,
        Resource::PbsDatastore(_) => None,
    }
}

/// Wrapper to 'locale compare' to strings
///
/// Note: The first parameter must be a [`String`], since it needs to be converted to a [`js_sys::JsString`].
/// The `numeric` parameter corresponds to the numeric parameter of `String.localeCompare` from
/// Javascript.
///
/// Seel also
/// https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/localeCompare
pub(crate) fn locale_compare(first: String, second: &str, numeric: bool) -> std::cmp::Ordering {
    let first: JsString = first.into();
    let options = Object::new();
    // TODO: find a better way to create the options object
    let _ = js_sys::Reflect::set(&options, &"numeric".into(), &numeric.into());
    first
        .locale_compare(second, &Array::new(), &options)
        .cmp(&0)
}

/// Returns true if the global subscription checks succeeded
pub async fn check_subscription() -> bool {
    let data: Result<Value, _> = http_get("/nodes/localhost/subscription", None).await;
    proxmox_yew_comp::subscription_is_active(Some(&data))
}

/// Extract the version of a specific package from `RemoteUpdateSummary` for a specific node
pub fn extract_package_version(
    updates: &RemoteUpdateSummary,
    node: &str,
    package_name: &str,
) -> Option<Version> {
    let entry = updates.nodes.get(node)?;
    let version = entry
        .versions
        .iter()
        .find_map(|package| (package.package == package_name).then_some(package.version.clone()))?;

    version.parse().ok()
}
