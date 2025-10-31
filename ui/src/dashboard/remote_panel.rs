use std::rc::Rc;

use yew::html::IntoEventCallback;
use yew::virtual_dom::{VComp, VNode};

use pdm_search::{Search, SearchTerm};
use proxmox_yew_comp::Status;
use pwt::css;
use pwt::prelude::*;
use pwt::props::{ContainerBuilder, WidgetBuilder};
use pwt::widget::menu::{Menu, MenuButton, MenuEvent, MenuItem};
use pwt::widget::{Column, Container, Fa, Panel};

use pdm_api_types::resource::ResourcesStatus;

use crate::{dashboard::create_title_with_icon, search_provider::get_search_provider};

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
                tr!("Failed to reach one remote." | "Failed to reach {n} remotes." % failed),
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

pub fn create_remote_panel(
    status: Option<ResourcesStatus>,
    on_pve_wizard: impl IntoEventCallback<MenuEvent>,
    on_pbs_wizard: impl IntoEventCallback<MenuEvent>,
) -> Panel {
    Panel::new()
        .title(create_title_with_icon("server", tr!("Remotes")))
        .border(true)
        .with_tool(
            MenuButton::new(tr!("Add")).show_arrow(true).menu(
                Menu::new()
                    .with_item(
                        MenuItem::new("Proxmox VE")
                            .icon_class("fa fa-building")
                            .on_select(on_pve_wizard),
                    )
                    .with_item(
                        MenuItem::new("Proxmox Backup Server")
                            .icon_class("fa fa-floppy-o")
                            .on_select(on_pbs_wizard),
                    ),
            ),
        )
        .with_child(RemotePanel::new(status))
}
