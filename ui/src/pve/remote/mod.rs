use std::rc::Rc;

use proxmox_yew_comp::NotesView;
use yew::virtual_dom::{VComp, VNode};

use pwt::css::{AlignItems, ColorScheme};
use pwt::prelude::*;
use pwt::props::{ContainerBuilder, WidgetBuilder};
use pwt::widget::{Fa, Row, TabBarItem, TabPanel};

use crate::remotes::RemoteTaskList;

#[derive(Clone, Debug, Eq, PartialEq, Properties)]
pub struct PveRemotePanel {
    /// The remote to show
    pub remote: String,
}

impl PveRemotePanel {
    pub fn new(remote: String) -> Self {
        yew::props!(Self { remote })
    }
}

impl From<PveRemotePanel> for VNode {
    fn from(val: PveRemotePanel) -> Self {
        VComp::new::<PveRemotePanelComp>(Rc::new(val), None).into()
    }
}

struct PveRemotePanelComp;

impl yew::Component for PveRemotePanelComp {
    type Message = ();
    type Properties = PveRemotePanel;

    fn create(_ctx: &yew::Context<Self>) -> Self {
        Self
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();

        let title: Html = Row::new()
            .gap(2)
            .class(AlignItems::Baseline)
            .with_child(Fa::new("building"))
            .with_child(tr! {"Remote '{0}'", props.remote})
            .into();

        TabPanel::new()
            .router(true)
            .class(pwt::css::FlexFit)
            .title(title)
            .class(ColorScheme::Neutral)
            .with_item_builder(
                TabBarItem::new()
                    .key("tasks_view")
                    .label(tr!("Remote Tasks"))
                    .icon_class("fa fa-list"),
                {
                    let remote = props.remote.clone();
                    move |_| RemoteTaskList::new().remote(remote.clone()).into()
                },
            )
            .with_item_builder(
                TabBarItem::new()
                    .key("notes_view")
                    .label(tr!("Notes"))
                    .icon_class("fa fa-sticky-note-o"),
                {
                    let remote = props.remote.clone();
                    move |_| {
                        NotesView::edit_property(
                            format!("/pve/remotes/{remote}/options"),
                            "description",
                        )
                        .on_submit(None)
                        .into()
                    }
                },
            )
            .into()
    }
}
