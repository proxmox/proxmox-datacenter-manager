use pwt::css;
use pwt::prelude::*;
use pwt::widget::{Column, Fa, Row};

mod top_entities;
pub use top_entities::create_top_entities_panel;

mod subscription_info;
pub use subscription_info::create_subscription_panel;

mod subscriptions_list;
pub use subscriptions_list::SubscriptionsList;

mod remote_panel;
pub use remote_panel::create_remote_panel;

mod guest_panel;
pub use guest_panel::create_guest_panel;

mod node_status_panel;
use node_status_panel::create_node_panel;

mod sdn_zone_panel;
pub use sdn_zone_panel::create_sdn_panel;

mod status_row;
pub use status_row::DashboardStatusRow;

mod filtered_tasks;

mod pbs_datastores_panel;
pub use pbs_datastores_panel::create_pbs_datastores_panel;

mod tasks;
pub use tasks::create_task_summary_panel;

pub mod types;

pub mod view;

mod refresh_config_edit;
pub use refresh_config_edit::create_refresh_config_edit_window;

fn loading_column() -> Column {
    Column::new()
        .padding(4)
        .class(css::FlexFit)
        .class(css::JustifyContent::Center)
        .class(css::AlignItems::Center)
        .with_child(html! {<i class={"pwt-loading-icon"} />})
}

/// Create a consistent title component for the given title and icon
fn create_title_with_icon(icon: &str, title: String) -> Html {
    Row::new()
        .class(css::AlignItems::Center)
        .gap(2)
        .with_child(Fa::new(icon))
        .with_child(title)
        .into()
}
