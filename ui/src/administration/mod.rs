use pwt::prelude::*;

use pwt::state::NavigationContainer;
use pwt::widget::{MiniScrollMode, TabBarItem, TabPanel};

//mod server_status;
//pub use server_status::ServerStatus;

//mod services;
//pub use services::Services;

use proxmox_yew_comp::{AptPackageManager, AptRepositories, ExistingProduct, Syslog, Tasks};

#[function_component(ServerAdministration)]
pub fn server_administration() -> Html {
    let panel = TabPanel::new()
        .state_id("*ServerAdministrationState")
        .class("pwt-flex-fill pwt-overflow-auto")
        .title("Server Administration")
        .router(true)
        .scroll_mode(MiniScrollMode::Arrow)
        /*
        .with_item_builder(
            TabBarItem::new()
                .key("server-status")
                .label("Server status")
                .icon_class("fa fa-area-chart"),
            |_| html! { <ServerStatus/> },
        )
        .with_item_builder(
            TabBarItem::new()
                .key("service")
                .label("Services")
                .icon_class("fa fa-gears"),
            |_| Services::new().into(),
        )
        */
        .with_item_builder(
            TabBarItem::new()
                .key("updates")
                .label("Updates")
                .icon_class("fa fa-refresh"),
            |_| AptPackageManager::new().into(),
        )
        .with_item_builder(
            TabBarItem::new()
                .key("repositories")
                .label("Repositories")
                .icon_class("fa fa-files-o"),
            |_| AptRepositories::new().product(ExistingProduct::PDM).into(),
        )
        .with_item_builder(
            TabBarItem::new()
                .key("syslog")
                .label("Syslog")
                .icon_class("fa fa-list"),
            |_| Syslog::new().into(), // fixme: use JournalView instead?
        )
        .with_item_builder(
            TabBarItem::new()
                .key("tasks")
                .label("Tasks")
                .icon_class("fa fa-list-alt"),
            |_| Tasks::new().into(),
        );

    NavigationContainer::new().with_child(panel).into()
}
