mod wizard_page_connect;
use wizard_page_connect::WizardPageConnect;

mod wizard_page_nodes;
use wizard_page_nodes::WizardPageNodes;

mod wizard_page_summary;
pub use wizard_page_summary::WizardPageSummary;

mod wizard_page_info;
pub use wizard_page_info::WizardPageInfo;

mod add_wizard;
pub use add_wizard::AddWizard;

mod node_url_list;
pub use node_url_list::NodeUrlList;

mod edit_remote;

mod config;
pub use config::{create_remote, RemoteConfigPanel};

mod tasks;

use yew::{function_component, Html};

use pwt::{
    props::{ContainerBuilder, StorageLocation},
    state::NavigationContainer,
    widget::{MiniScrollMode, TabBarItem, TabPanel},
};

#[function_component(RemotesPanel)]
pub fn system_configuration() -> Html {
    let panel = TabPanel::new()
        .state_id(StorageLocation::session("RemotesPanelState"))
        .class(pwt::css::FlexFit)
        .router(true)
        .scroll_mode(MiniScrollMode::Arrow)
        .with_item_builder(
            TabBarItem::new()
                .key("configuration")
                .label("Configuration")
                .icon_class("fa fa-cogs"),
            |_| RemoteConfigPanel::new().into(),
        )
        .with_item_builder(
            TabBarItem::new()
                .key("tasks")
                .label("Tasks")
                .icon_class("fa fa-book"),
            |_| tasks::RemoteTaskList::new().into(),
        );

    NavigationContainer::new().with_child(panel).into()
}
