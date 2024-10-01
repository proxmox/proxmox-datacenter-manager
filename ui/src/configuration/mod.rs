use pwt::prelude::*;
use pwt::state::NavigationContainer;
use pwt::widget::{MiniScrollMode, TabBarItem, TabPanel};

use proxmox_yew_comp::configuration::TimePanel;
use proxmox_yew_comp::configuration::{DnsPanel, NetworkView};
use proxmox_yew_comp::tfa::TfaView;
use proxmox_yew_comp::UserPanel;

#[function_component(SystemConfiguration)]
pub fn system_configuration() -> Html {
    let panel = TabPanel::new()
        .state_id("*SystemConfigurationState")
        .title(tr!("Configuration") + ": " + &tr!("System"))
        .class("pwt-flex-fit")
        .router(true)
        .scroll_mode(MiniScrollMode::Arrow)
        .with_item_builder(
            TabBarItem::new()
                .key("network")
                .icon_class("fa fa-exchange")
                .label(tr!("Network")),
            |_| NetworkView::new().into(),
        )
        .with_item_builder(
            TabBarItem::new()
                .key("dns")
                .icon_class("fa fa-globe")
                .label("DNS"),
            |_| html! { <DnsPanel/> },
        )
        .with_item_builder(
            TabBarItem::new()
                .key("time")
                .icon_class("fa fa-clock-o")
                .label(tr!("Time")),
            |_| html! { <TimePanel/> },
        );

    NavigationContainer::new().with_child(panel).into()
}

#[function_component(AccessControl)]
pub fn access_control() -> Html {
    let panel = TabPanel::new()
        .state_id("*AccessControlState")
        .title(tr!("Configuration") + ": " + &tr!("Access Control"))
        .class("pwt-flex-fit")
        .router(true)
        .scroll_mode(MiniScrollMode::Arrow)
        .with_item_builder(
            TabBarItem::new()
                .key("user-management")
                .icon_class("fa fa-user")
                .label(tr!("User Management")),
            |_| UserPanel::new().into(),
        )
        .with_item_builder(
            TabBarItem::new()
                .key("two-factor")
                .icon_class("fa fa-key")
                .label(tr!("Two Factor Authentication")),
            |_| TfaView::new().into(),
        );

    NavigationContainer::new().with_child(panel).into()
}
