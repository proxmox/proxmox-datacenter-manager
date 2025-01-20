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
