use std::rc::Rc;

use pdm_search::{Search, SearchTerm};
use proxmox_yew_comp::Status;
use pwt::{
    css,
    prelude::*,
    props::{ContainerBuilder, WidgetBuilder},
    widget::{Column, Container, Fa},
};
use yew::{
    virtual_dom::{VComp, VNode},
    Component, Properties,
};

use pdm_api_types::resource::ResourcesStatus;

use crate::search_provider::get_search_provider;

#[derive(Properties, PartialEq)]
/// A panel for showing the overall remotes status
pub struct RemotePanel {
    /// The status loaded from the API
    pub status: Option<ResourcesStatus>,
}

impl RemotePanel {
    /// Takes the status of the API, or None (which indicates loading)
    pub fn new(status: Option<ResourcesStatus>) -> Self {
        yew::props!(Self { status })
    }
}

impl From<RemotePanel> for VNode {
    fn from(val: RemotePanel) -> Self {
        let comp = VComp::new::<PdmRemotePanel>(Rc::new(val), None);
        VNode::from(comp)
    }
}

struct PdmRemotePanel {}

impl Component for PdmRemotePanel {
    type Message = Search;
    type Properties = RemotePanel;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self {}
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        if let Some(search) = get_search_provider(ctx) {
            search.search(msg);
        }
        false
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();
        if props.status.is_none() {
            return Column::new()
                .padding(4)
                .class(css::FlexFit)
                .class(css::AlignItems::Center)
                .class(css::JustifyContent::Center)
                .with_child(Fa::new("").class("pwt-loading-icon"))
                .into();
        }
        let status = props.status.clone().unwrap();

        let (remote_icon, remote_text, failure) = match (status.failed_remotes, status.remotes) {
            (0, 0) => (
                Fa::from(Status::Warning),
                tr!("No remotes configured."),
                false,
            ),
            (0, _) => (
                Fa::from(Status::Success),
                tr!("Could reach all remotes."),
                false,
            ),
            (failed, _) => (
                Fa::from(Status::Error),
                tr!("{0} remotes failed to reach.", failed),
                true,
            ),
        };
        Column::new()
            .tabindex(if failure { 0 } else { -1 })
            .onclick(ctx.link().callback(move |_| create_search_term(failure)))
            .onkeydown(ctx.link().batch_callback(move |event: KeyboardEvent| {
                match event.key().as_str() {
                    "Enter" | " " => Some(create_search_term(failure)),
                    _ => None,
                }
            }))
            .padding(4)
            .class(css::FlexFit)
            .class(css::AlignItems::Center)
            .class(css::JustifyContent::Center)
            .style("cursor", failure.then_some("pointer"))
            .gap(2)
            .with_child(remote_icon.large_4x())
            .with_child(Container::new().with_child(remote_text))
            .into()
    }
}

fn create_search_term(failure: bool) -> Search {
    if failure {
        Search::with_terms(vec![
            SearchTerm::new("remote").category(Some("type")),
            SearchTerm::new("offline").category(Some("status")),
        ])
    } else {
        Search::with_terms(vec![SearchTerm::new("remote").category(Some("type"))])
    }
}
