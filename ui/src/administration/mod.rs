use std::rc::Rc;

use html::IntoPropValue;
use pwt::prelude::*;
use pwt::props::StorageLocation;
use yew::virtual_dom::{VComp, VNode};

use pwt::state::NavigationContainer;
use pwt::widget::{MiniScrollMode, TabBarItem, TabPanel};

use pwt_macros::builder;

//mod server_status;
//pub use server_status::ServerStatus;

//mod services;
//pub use services::Services;

use proxmox_yew_comp::{AptPackageManager, AptRepositories, ExistingProduct, Syslog, Tasks};

use crate::widget::ContentSpacer;

#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct ServerAdministration {
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub username: Option<AttrValue>,
}

impl ServerAdministration {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

pub struct PdmServerAdministration {}

impl Component for PdmServerAdministration {
    type Message = ();
    type Properties = ServerAdministration;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {}
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let enable_upgrade = ctx.props().username.as_deref() == Some("root@pam");

        let panel = TabPanel::new()
            .state_id(StorageLocation::session("ServerAdministrationState"))
            .class(pwt::css::FlexFit)
            //.title("Server Administration")
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
                move |_| {
                    ContentSpacer::new()
                        .class(pwt::css::FlexFit)
                        .with_child(AptPackageManager::new().enable_upgrade(enable_upgrade))
                        .into()
                },
            )
            .with_item_builder(
                TabBarItem::new()
                    .key("repositories")
                    .label("Repositories")
                    .icon_class("fa fa-files-o"),
                |_| {
                    ContentSpacer::new()
                        .class(pwt::css::FlexFit)
                        .with_child(AptRepositories::new().product(ExistingProduct::PDM))
                        .into()
                },
            )
            .with_item_builder(
                TabBarItem::new()
                    .key("syslog")
                    .label("Syslog")
                    .icon_class("fa fa-list"),
                |_| {
                    ContentSpacer::new()
                        .class(pwt::css::FlexFit)
                        .with_child(Syslog::new())
                        .into() // fixme: use JournalView instead?
                },
            )
            .with_item_builder(
                TabBarItem::new()
                    .key("tasks")
                    .label("Tasks")
                    .icon_class("fa fa-list-alt"),
                |_| {
                    ContentSpacer::new()
                        .class(pwt::css::FlexFit)
                        .with_child(Tasks::new())
                        .into()
                },
            );

        NavigationContainer::new().with_child(panel).into()
    }
}

impl Into<VNode> for ServerAdministration {
    fn into(self) -> VNode {
        let comp = VComp::new::<PdmServerAdministration>(Rc::new(self), None);
        VNode::from(comp)
    }
}
