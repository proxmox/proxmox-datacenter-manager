use std::rc::Rc;

use yew::virtual_dom::{Key, VComp, VNode};

use pwt::prelude::*;
use pwt::state::{Loader, Selection};
use pwt::widget::nav::{Menu, MenuItem, NavigationDrawer};
use pwt::widget::{Container, Panel, Row, SelectionView, SelectionViewRenderInfo};

use proxmox_yew_comp::common_api_types::TaskListItem;
use proxmox_yew_comp::{NotesView, XTermJs};

use crate::{CertificatesPanel, RemoteConfigPanel, ServerAdministration, SystemConfiguration};

/*
use crate::{
    AccessControl, Dashboard, PbsDatastorePanel, PbsDatastoreRootPanel, PbsTapePanel,
    ServerAdministration, SystemConfiguration, XtermJsConsole,
};

use crate::configuration::{RemoteConfigPanel, TrafficControlView};
use crate::certificates::CertificatesPanel;

*/

#[derive(Clone, PartialEq, Properties)]
pub struct MainMenu {
    running_tasks: Loader<Vec<TaskListItem>>,
}

impl MainMenu {
    pub fn new(running_tasks: Loader<Vec<TaskListItem>>) -> Self {
        yew::props!(Self { running_tasks })
    }
}

pub enum Msg {
    Select(Key),
}

pub struct PdmMainMenu {
    active: Key,
    menu_selection: Selection,
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

fn register_submenu(
    menu: &mut Menu,
    view: &mut SelectionView,
    text: impl Into<String>,
    id: &str,
    icon_class: Option<&'static str>,
    renderer: impl 'static + Fn(&SelectionViewRenderInfo) -> Html,
    submenu: Menu,
) {
    view.add_builder(id, renderer);
    menu.add_item(
        MenuItem::new(text.into())
            .key(id.to_string())
            .icon_class(icon_class)
            .submenu(submenu),
    );
}

impl Component for PdmMainMenu {
    type Message = Msg;
    type Properties = MainMenu;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            active: Key::from("certificates"),
            menu_selection: Selection::new(),
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Select(key) => {
                self.active = key;
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let scope = ctx.link().clone();

        let mut content = SelectionView::new()
            .class("pwt-fit")
            .selection(self.menu_selection.clone());

        let mut menu = Menu::new();

        register_view(
            &mut menu,
            &mut content,
            tr!("Dashboard"),
            "dashboard",
            Some("fa fa-tachometer"),
            move |_| html! {"Dashboard"},
        );

        register_view(
            &mut menu,
            &mut content,
            tr!("Notes"),
            "notes",
            Some("fa fa-sticky-note-o"),
            move |_| {
                Panel::new()
                    .class("pwt-flex-fit")
                    .border(false)
                    .title(tr!("Notes"))
                    .with_child(NotesView::new())
                    .into()
            },
        );

        let mut config_submenu = Menu::new();

        register_view(
            &mut config_submenu,
            &mut content,
            tr!("Access Control"),
            "access",
            Some("fa fa-key"),
            //|_| html! { <AccessControl /> },
            |_| html! { "AccessControl" },
        );

        register_view(
            &mut config_submenu,
            &mut content,
            tr!("Remotes"),
            "remotes",
            Some("fa fa-server"),
            |_| RemoteConfigPanel::new().into(),
        );

        register_view(
            &mut config_submenu,
            &mut content,
            tr!("Certificates"),
            "certificates",
            Some("fa fa-certificate"),
            |_| html! {<CertificatesPanel/>},
        );

        register_submenu(
            &mut menu,
            &mut content,
            tr!("Configuration"),
            "configuration",
            Some("fa fa-gears"),
            |_| html! { <SystemConfiguration/> },
            config_submenu,
        );

        let mut admin_submenu = Menu::new();

        register_view(
            &mut admin_submenu,
            &mut content,
            tr!("Shell"),
            "shell",
            Some("fa fa-terminal"),
            |_| XTermJs::new().into(),
        );

        register_submenu(
            &mut menu,
            &mut content,
            tr!("Administration"),
            "administration",
            Some("fa fa-wrench"),
            |_| html! { <ServerAdministration/> },
            admin_submenu,
        );

        let drawer = NavigationDrawer::new(menu)
            .aria_label("Datacenter Manager")
            .class("pwt-border-end")
            .router(true)
            .default_active(self.active.to_string())
            .selection(self.menu_selection.clone())
            .on_select(Callback::from(move |id: Option<Key>| {
                let id = id.unwrap_or_else(|| Key::from(""));
                scope.send_message(Msg::Select(id))
            }));

        Container::new()
            .class("pwt-d-flex pwt-flex-fit")
            .with_child(
                Row::new()
                    .class("pwt-flex-fit")
                    .with_child(drawer)
                    .with_child(content),
            )
            .into()
    }
}

impl Into<VNode> for MainMenu {
    fn into(self) -> VNode {
        let comp = VComp::new::<PdmMainMenu>(Rc::new(self), None);
        VNode::from(comp)
    }
}
