use pwt::prelude::*;
use pwt::props::StorageLocation;
use pwt::state::NavigationContainer;
use pwt::widget::{MiniScrollMode, Panel, TabBarItem, TabPanel};

use proxmox_yew_comp::configuration::TimePanel;
use proxmox_yew_comp::configuration::{DnsPanel, NetworkView};
use proxmox_yew_comp::tfa::TfaView;
use proxmox_yew_comp::UserPanel;

mod other;
pub use other::OtherPanel;

use crate::widget::ContentSpacer;

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
                .label(tr!("Network") + "/" + &tr!("Time"))
                .icon_class("fa fa-exchange"),
            |_| html! { <NetworkTimePanel/> },
        )
        .with_item_builder(
            TabBarItem::new()
                .key("other")
                .label("Other")
                .icon_class("fa fa-sliders"),
            |_| html! { <OtherPanel/> },
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
                ContentSpacer::new()
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
                ContentSpacer::new()
                    .class(pwt::css::FlexFit)
                    .with_child(TfaView::new())
                    .into()
            },
        );

    NavigationContainer::new().with_child(panel).into()
}

#[function_component(NetworkTimePanel)]
pub fn create_network_time_panel() -> Html {
    ContentSpacer::new()
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
