use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;
use yew::virtual_dom::{Key, VComp, VNode};
use yew::Callback;
use yew::Html;
use yew::Properties;

use proxmox_yew_comp::{LoadableComponent, LoadableComponentContext, LoadableComponentMaster};
use pwt::css::FlexFit;
use pwt::props::{ContainerBuilder, WidgetBuilder};
use pwt::state::{NavigationContainer, PersistentState, Selection};
use pwt::widget::nav::{Menu, MenuItem, NavigationDrawer};
use pwt::widget::{Pane, SplitPane};
use pwt::widget::{SelectionView, SelectionViewRenderInfo};

use pbs_api_types::DataStoreConfig;

mod snapshot_list;
pub use snapshot_list::SnapshotList;

#[derive(PartialEq, Properties)]
pub struct DatastoreMenu {
    remote: String,
}

impl DatastoreMenu {
    pub fn new(remote: String) -> Self {
        yew::props!(Self { remote })
    }
}

impl Into<VNode> for DatastoreMenu {
    fn into(self) -> VNode {
        let comp = VComp::new::<LoadableComponentMaster<PbsDatastoreMenu>>(Rc::new(self), None);
        VNode::from(comp)
    }
}

pub struct PbsDatastoreMenu {
    datastore_list: PersistentState<Vec<DataStoreConfig>>,
    active: Key,
    selection: Selection,
}

pub enum Msg {
    Select(Key),
    UpdateDatastoreList(Vec<DataStoreConfig>),
}

#[derive(PartialEq)]
pub enum ViewState {}

impl LoadableComponent for PbsDatastoreMenu {
    type Message = Msg;
    type Properties = DatastoreMenu;
    type ViewState = ViewState;

    fn load(
        &self,
        ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        let remote = ctx.props().remote.clone();
        let link = ctx.link();
        Box::pin(async move {
            let mut data = crate::pdm_client().pbs_list_datastores(&remote).await?;
            data.sort_by(|a, b| a.name.cmp(&b.name));
            link.send_message(Msg::UpdateDatastoreList(data));
            Ok(())
        })
    }

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        let props = ctx.props();
        let link = ctx.link();
        link.repeated_load(3000);

        Self {
            datastore_list: PersistentState::new(format!("PdmPbsDatastoreList-{}", props.remote)),
            selection: Selection::new(),
            active: Key::from(""),
        }
    }

    fn update(&mut self, _ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Select(key) => {
                self.active = key;
                true
            }
            Msg::UpdateDatastoreList(list) => {
                self.datastore_list.update(list);
                true
            }
        }
    }

    fn toolbar(&self, _ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        None
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        let scope = ctx.link().clone();
        let props = ctx.props();

        let mut content = SelectionView::new()
            .class(FlexFit)
            .selection(self.selection.clone());

        let mut menu = Menu::new();

        for datastore in self.datastore_list.iter() {
            register_view(
                &mut menu,
                &mut content,
                &datastore.name,
                &datastore.name,
                Some("fa fa-server"),
                {
                    let datastore = datastore.name.clone();
                    let remote = props.remote.clone();
                    move |_| SnapshotList::new(remote.clone(), datastore.clone()).into()
                },
            );
        }

        let drawer = NavigationDrawer::new(menu)
            .aria_label("PBS Datastore Menu")
            .class("pwt-border-end")
            .router(true)
            // .default_active(self.active.to_string())
            .selection(self.selection.clone())
            .on_select(Callback::from(move |id: Option<Key>| {
                let id = id.unwrap_or_else(|| Key::from(""));
                scope.send_message(Msg::Select(id))
            }));

        NavigationContainer::new()
            .with_child(
                SplitPane::new()
                    .class(FlexFit)
                    .with_child(Pane::new(drawer).size(None))
                    .with_child(Pane::new(content).flex(1)),
            )
            .into()
    }

    fn dialog_view(
        &self,
        _ctx: &LoadableComponentContext<Self>,
        _view_state: &Self::ViewState,
    ) -> Option<Html> {
        None
    }
}

fn register_view(
    menu: &mut Menu,
    view: &mut SelectionView,
    text: impl Into<String>,
    id: &str,
    icon_class: Option<&'static str>,
    renderer: impl 'static + Fn(&SelectionViewRenderInfo) -> Html,
) {
    view.add_builder(id, renderer);
    menu.add_item(
        MenuItem::new(text.into())
            .key(id.to_string())
            .icon_class(icon_class),
    );
}
