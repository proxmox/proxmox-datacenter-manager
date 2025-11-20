use std::future::Future;
use std::rc::Rc;

use anyhow::Error;
use gloo_utils::window;
use pbs_api_types::DataStoreConfig;
use yew::virtual_dom::{VComp, VNode};
use yew::{Html, Properties};

use proxmox_yew_comp::{
    ConsoleType, LoadableComponent, LoadableComponentContext, LoadableComponentMaster, XTermJs,
};
use pwt::css::{AlignItems, FlexFit};
use pwt::prelude::*;
use pwt::state::NavigationContainer;
use pwt::tr;
use pwt::widget::{Button, Column, Container, Fa, Panel, Row};

mod tree;

mod datastore;
pub use datastore::DatastorePanel;

mod namespace_selector;

mod node;
use node::overview::RemoteOverviewPanel;

mod snapshot_list;
pub use snapshot_list::SnapshotList;

use crate::pbs::tree::PbsTree;
use crate::{get_deep_url, pdm_client};

#[derive(Debug, Eq, PartialEq, Properties)]
pub struct PbsRemote {
    remote: String,
}

impl PbsRemote {
    pub fn new(remote: String) -> Self {
        Self { remote }
    }
}

impl From<PbsRemote> for VNode {
    fn from(val: PbsRemote) -> Self {
        VComp::new::<LoadableComponentMaster<PbsRemoteComp>>(Rc::new(val), None).into()
    }
}

#[allow(clippy::large_enum_variant)]
pub enum Msg {
    SelectedView(tree::PbsTreeNode),
    ResourcesList(Vec<DataStoreConfig>),
}

#[doc(hidden)]
pub struct PbsRemoteComp {
    datastores: Rc<Vec<DataStoreConfig>>,
    view: tree::PbsTreeNode,
}

impl PbsRemoteComp {
    async fn load_datastores(remote: &str) -> Result<Vec<DataStoreConfig>, Error> {
        let datastores = pdm_client().pbs_list_datastores(remote).await?;
        Ok(datastores)
    }
}

impl LoadableComponent for PbsRemoteComp {
    type Message = Msg;
    type Properties = PbsRemote;
    type ViewState = ();

    fn create(_ctx: &LoadableComponentContext<Self>) -> Self {
        Self {
            datastores: Rc::new(Vec::new()),
            view: tree::PbsTreeNode::Root,
        }
    }

    fn load(
        &self,
        _ctx: &LoadableComponentContext<Self>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), anyhow::Error>>>> {
        let link = _ctx.link().clone();
        let remote = _ctx.props().remote.clone();
        Box::pin(async move {
            link.send_message(Msg::ResourcesList(Self::load_datastores(&remote).await?));
            Ok(())
        })
    }

    fn update(&mut self, _ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::SelectedView(pbs_tree_node) => {
                self.view = pbs_tree_node;
            }
            Msg::ResourcesList(vec) => {
                self.datastores = Rc::new(vec);
            }
        }
        true
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> yew::Html {
        let props = ctx.props();

        let content: Html = match &self.view {
            tree::PbsTreeNode::Root => RemoteOverviewPanel::new(props.remote.clone()).into(),
            tree::PbsTreeNode::Datastore(data_store_config) => {
                DatastorePanel::new(props.remote.clone(), data_store_config.clone()).into()
            }
        };

        let title: Html = Row::new()
            .gap(2)
            .class(AlignItems::Center)
            .with_child(Fa::new("server"))
            .with_child(tr! {"Remote '{0}'", ctx.props().remote})
            .into();

        NavigationContainer::new()
            .with_child(
                Container::new()
                    .class("pwt-content-spacer")
                    .class("pwt-flex-direction-row")
                    .class(FlexFit)
                    .with_child(
                        Panel::new()
                            .border(true)
                            .class(FlexFit)
                            .title(title.clone())
                            .style("flex", "1 1 0")
                            .max_width(500)
                            .with_tool(
                                Button::new(tr!("Open Web UI"))
                                    .icon_class("fa fa-external-link")
                                    .on_activate({
                                        let link = ctx.link().clone();
                                        let remote = ctx.props().remote.clone();
                                        move |_| {
                                            if let Some(url) =
                                                get_deep_url(link.yew_link(), &remote, None, "")
                                            {
                                                let _ = window().open_with_url(&url.href());
                                            }
                                        }
                                    }),
                            )
                            .with_tool(
                                Button::new(tr!("Open Shell"))
                                    .icon_class("fa fa-terminal")
                                    .on_activate({
                                        let remote = ctx.props().remote.clone();
                                        move |_| {
                                            XTermJs::open_xterm_js_viewer(
                                                ConsoleType::RemotePbsLoginShell(remote.clone()),
                                                "localhost",
                                                false,
                                            )
                                        }
                                    }),
                            )
                            .with_child(Column::new().padding(4).class(FlexFit).with_child(
                                PbsTree::new(
                                    props.remote.clone(),
                                    self.datastores.clone(),
                                    ctx.loading(),
                                    ctx.link().callback(Msg::SelectedView),
                                    {
                                        let link = ctx.link().clone();
                                        move |_| link.send_reload()
                                    },
                                ),
                            )),
                    )
                    .with_child(
                        Panel::new()
                            .style("flex", "2 1 0")
                            .border(true)
                            .class(FlexFit)
                            .with_child(content),
                    ),
            )
            .into()
    }
}
