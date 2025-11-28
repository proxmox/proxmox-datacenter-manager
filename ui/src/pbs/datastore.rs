use std::rc::Rc;

use yew::{
    virtual_dom::{VComp, VNode},
    Component, Properties,
};

use pwt::{css::FlexFit, props::WidgetBuilder, tr, widget::TabBarItem};

use pbs_api_types::DataStoreConfig;

use crate::pbs::SnapshotList;

mod overview;
use overview::DataStoreOverview;

#[derive(Properties, PartialEq)]
pub struct DatastorePanel {
    remote: String,
    config: DataStoreConfig,
}

impl DatastorePanel {
    pub fn new(remote: String, config: DataStoreConfig) -> Self {
        yew::props!(Self { remote, config })
    }
}

impl From<DatastorePanel> for VNode {
    fn from(val: DatastorePanel) -> Self {
        VComp::new::<DatastorePanelComp>(Rc::new(val), None).into()
    }
}

#[doc(hidden)]
struct DatastorePanelComp {}

impl Component for DatastorePanelComp {
    type Message = ();
    type Properties = DatastorePanel;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self {}
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();
        pwt::widget::TabPanel::new()
            .router(true)
            .class(FlexFit)
            .title(tr!("Datastore {0}", props.config.name))
            .with_item_builder(
                TabBarItem::new()
                    .key("overview")
                    .label(tr!("Overview"))
                    .icon_class("fa fa-tachometer"),
                {
                    let remote = props.remote.clone();
                    let config = props.config.clone();
                    move |_| DataStoreOverview::new(remote.clone(), config.clone()).into()
                },
            )
            .with_item_builder(
                TabBarItem::new()
                    .key("content")
                    .label(tr!("Content"))
                    .icon_class("fa fa-th"),
                {
                    let remote = props.remote.clone();
                    let name = props.config.name.clone();
                    move |_| SnapshotList::new(remote.clone(), name.clone()).into()
                },
            )
            .into()
    }
}
