use pwt::prelude::*;
use pwt::props::StorageLocation;
use pwt::state::NavigationContainer;
use pwt::widget::{Container, MiniScrollMode, Panel, TabBarItem, TabPanel};

use proxmox_yew_comp::configuration::TimePanel;
use proxmox_yew_comp::configuration::{DnsPanel, NetworkView};
use proxmox_yew_comp::tfa::TfaView;
use proxmox_yew_comp::UserPanel;

mod webauthn;
pub use webauthn::WebauthnPanel;

#[function_component(SystemConfiguration)]
pub fn system_configuration() -> Html {
    let panel = TabPanel::new()
        .state_id(StorageLocation::session("SystemConfigurationState"))
        //.title(tr!("Configuration") + ": " + &tr!("System"))
        .class(pwt::css::FlexFit)
        .router(true)
        .scroll_mode(MiniScrollMode::Arrow)
        .with_item_builder(
            TabBarItem::new()
                .key("network")
                // TRANSLATORS: As in "Network and Time system configuration"
                .label(tr!("Network/Time"))
                .icon_class("fa fa-exchange"),
            |_| html! { <NetworkTimePanel/> },
        )
        .with_item_builder(
            TabBarItem::new()
                .key("webauthn")
                .label(tr!("WebAuthn TFA"))
                .icon_class("fa fa-sliders"),
            |_| html! { <WebauthnPanel/> },
        );

    NavigationContainer::new().with_child(panel).into()
}

#[function_component(AccessControl)]
pub fn access_control() -> Html {
    let panel = TabPanel::new()
        .state_id(StorageLocation::session("AccessControlState"))
        //.title(tr!("Configuration") + ": " + &tr!("Access Control"))
        .class(pwt::css::FlexFit)
        .router(true)
        .scroll_mode(MiniScrollMode::Arrow)
        .with_item_builder(
            TabBarItem::new()
                .key("user-management")
                .icon_class("fa fa-user")
                .label(tr!("User Management")),
            |_| {
                Container::new()
                    .class("pwt-content-spacer")
                    .class(pwt::css::FlexFit)
                    .with_child(UserPanel::new())
                    .into()
            },
        )
        .with_item_builder(
            TabBarItem::new()
                .key("two-factor")
                .icon_class("fa fa-key")
                .label(tr!("Two Factor Authentication")),
            |_| {
                Container::new()
                    .class("pwt-content-spacer")
                    .class(pwt::css::FlexFit)
                    .with_child(TfaView::new())
                    .into()
            },
        );

    NavigationContainer::new().with_child(panel).into()
}

#[function_component(NetworkTimePanel)]
pub fn create_network_time_panel() -> Html {
    Container::new()
        .class("pwt-content-spacer")
        .class(pwt::css::FlexFit)
        .with_child(
            Panel::new()
                .title(tr!("Time"))
                .with_child(html! { <TimePanel/> }),
        )
        .with_child(
            Panel::new()
                .title(tr!("DNS"))
                .with_child(html! { <DnsPanel/> }),
        )
        .with_child(
            Panel::new()
                .min_height(200)
                .class(pwt::css::FlexFit)
                .title(tr!("Network Interfaces"))
                .with_child(NetworkView::new()),
        )
        .into()
}
