use pdm_api_types::resource::{PveLxcResource, PveQemuResource};
use serde::{Deserialize, Serialize};

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

mod dashboard;
pub use dashboard::Dashboard;
use yew_router::prelude::RouterScopeExt;

mod widget;

pub mod pbs;
pub mod pve;

pub mod renderer;

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
    id: &str,
) -> Option<web_sys::Url> {
    let remote = get_remote(link, remote)?;
    let (default_port, hash) = match remote.ty {
        pdm_api_types::remotes::RemoteType::Pve => (
            "8006",
            if id.is_empty() {
                String::new()
            } else {
                format!("v1::={id}")
            },
        ),
        pdm_api_types::remotes::RemoteType::Pbs => (
            "8007",
            if id.is_empty() {
                String::new()
            } else {
                format!("DataStore-{id}")
            },
        ),
    };
    let node = remote.nodes.first()?;
    let url = web_sys::Url::new(&format!("https://{}/", node.hostname));
    if let Ok(url) = url {
        if url.port() == "" {
            url.set_port(default_port);
        }
        url.set_hash(&hash);
        Some(url)
    } else {
        None
    }
}

pub(crate) fn navigate_to<C: yew::Component>(
    link: &yew::html::Scope<C>,
    remote: &str,
    resource: Option<&pdm_client::types::Resource>,
) {
    if let Some(nav) = link.navigator() {
        let id = resource
            .and_then(|resource| {
                Some(match resource {
                    pdm_client::types::Resource::PveQemu(PveQemuResource { vmid, .. })
                    | pdm_client::types::Resource::PveLxc(PveLxcResource { vmid, .. }) => {
                        format!("guest+{vmid}")
                    }
                    pdm_client::types::Resource::PveNode(node) => format!("node+{}", node.node),
                    pdm_client::types::Resource::PbsDatastore(store) => store.name.clone(),
                    // FIXME: implement
                    _ => return None,
                })
            })
            .unwrap_or_default();
        nav.push(&yew_router::AnyRoute::new(format!("/remote-{remote}/{id}")));
    }
}
