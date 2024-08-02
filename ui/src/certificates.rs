use pwt::prelude::*;
use pwt::state::NavigationContainer;
use pwt::widget::{MiniScrollMode, TabBarItem, TabPanel};

use proxmox_yew_comp::acme::{
    AcmeAccountsPanel, AcmeDomainsPanel, AcmePluginsPanel, CertificateList,
};

#[function_component(CertificatesPanel)]
pub fn certificates_panel() -> Html {
    let panel = TabPanel::new()
        .state_id("*CertificatesState")
        .class("pwt-flex-fit")
        .title("Certificates")
        .router(true)
        .scroll_mode(MiniScrollMode::Arrow)
        .with_item_builder(
            TabBarItem::new()
                .key("certificate_List")
                .label("Certificates"),
            |_| CertificateList::new().into(),
        )
        .with_item_builder(
            TabBarItem::new().key("acme_domains").label("ACME Domains"),
            |_| AcmeDomainsPanel::new().url("/config/certificate").into(),
        )
        .with_item_builder(
            TabBarItem::new()
                .key("acme_accounts")
                .label("ACME Accounts"),
            |_| AcmeAccountsPanel::new().into(),
        )
        .with_item_builder(
            TabBarItem::new()
                .key("acme_plugins")
                .label("Challenge Plugins"),
            |_| AcmePluginsPanel::new().into(),
        );

    NavigationContainer::new().with_child(panel).into()
}
