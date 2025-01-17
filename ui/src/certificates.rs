use pwt::prelude::*;
use pwt::props::StorageLocation;
use pwt::state::NavigationContainer;
use pwt::widget::{Container, MiniScrollMode, TabBarItem, TabPanel};

use proxmox_yew_comp::acme::{
    AcmeAccountsPanel, AcmeDomainsPanel, AcmePluginsPanel, CertificateList,
};

#[function_component(CertificatesPanel)]
pub fn certificates_panel() -> Html {
    let panel = TabPanel::new()
        .state_id(StorageLocation::session("CertificatesState"))
        .class(pwt::css::FlexFit)
        //.title("Certificates")
        .router(true)
        .scroll_mode(MiniScrollMode::Arrow)
        .with_item_builder(
            TabBarItem::new()
                .key("certificate_List")
                .label("Certificates"),
            |_| {
                Container::new()
                    .class("pwt-content-spacer")
                    .class(pwt::css::FlexFit)
                    .with_child(CertificateList::new())
                    .into()
            },
        )
        .with_item_builder(
            TabBarItem::new().key("acme_domains").label("ACME Domains"),
            |_| {
                Container::new()
                    .class("pwt-content-spacer")
                    .class(pwt::css::FlexFit)
                    .with_child(AcmeDomainsPanel::new().url("/config/certificate"))
                    .into()
            },
        )
        .with_item_builder(
            TabBarItem::new()
                .key("acme_accounts")
                .label("ACME Accounts"),
            |_| {
                Container::new()
                    .class("pwt-content-spacer")
                    .class(pwt::css::FlexFit)
                    .with_child(AcmeAccountsPanel::new())
                    .into()
            },
        )
        .with_item_builder(
            TabBarItem::new()
                .key("acme_plugins")
                .label("Challenge Plugins"),
            |_| {
                Container::new()
                    .class("pwt-content-spacer")
                    .class(pwt::css::FlexFit)
                    .with_child(AcmePluginsPanel::new())
                    .into()
            },
        );

    NavigationContainer::new().with_child(panel).into()
}
