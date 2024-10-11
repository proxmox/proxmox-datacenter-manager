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

pub mod pbs;
pub mod pve;

pub fn pdm_client() -> pdm_client::PdmClient<std::rc::Rc<proxmox_yew_comp::HttpClientWasm>> {
    pdm_client::PdmClient(proxmox_yew_comp::CLIENT.with(|c| std::rc::Rc::clone(&c.borrow())))
}

#[derive(Clone, PartialEq)]
pub struct RemoteList(Vec<pdm_client::types::Remote>);

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
