use std::rc::Rc;

use serde::{Deserialize, Serialize};

use html::IntoPropValue;
use wasm_bindgen::UnwrapThrowExt;
use yew::virtual_dom::{Key, VComp, VNode};

use pwt::css::{self, Display, FlexFit};
use pwt::prelude::*;
use pwt::state::{PersistentState, Selection};
use pwt::widget::nav::{Menu, MenuItem, NavigationDrawer};
use pwt::widget::{Container, Row, SelectionView, SelectionViewRenderInfo};

use proxmox_yew_comp::{NotesView, XTermJs};

use pdm_api_types::remotes::RemoteType;

use crate::{
    AccessControl, CertificatesPanel, Dashboard, RemoteConfigPanel, RemoteList,
    ServerAdministration, SystemConfiguration,
};

/*
use crate::{
    AccessControl, Dashboard, PbsDatastorePanel, PbsDatastoreRootPanel, PbsTapePanel,
    ServerAdministration, SystemConfiguration, XtermJsConsole,
};

use crate::configuration::{RemoteConfigPanel, TrafficControlView};
use crate::certificates::CertificatesPanel;

*/

use pwt_macros::builder;

#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct MainMenu {
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub username: Option<AttrValue>,

    /// If set, add a loading indicator to the remote menu.
    ///
    /// Just to indicate that the remote list may not be up to date.
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub remote_list_loading: bool,
}

impl MainMenu {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

pub enum Msg {
    Select(Key),
    RemoteListChanged(RemoteList),
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
struct RemoteListCacheEntry {
    ty: RemoteType,
    id: String,
}

pub struct PdmMainMenu {
    active: Key,
    menu_selection: Selection,
    remote_list_cache: PersistentState<Vec<RemoteListCacheEntry>>,
    _remote_list_observer: ContextHandle<RemoteList>,
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

impl PdmMainMenu {}

impl Component for PdmMainMenu {
    type Message = Msg;
    type Properties = MainMenu;

    fn create(ctx: &Context<Self>) -> Self {
        let (_remote_list, _remote_list_observer) = ctx
            .link()
            .context(ctx.link().callback(Msg::RemoteListChanged))
            .unwrap_throw();
        Self {
            active: Key::from("dashboard"),
            menu_selection: Selection::new(),
            remote_list_cache: PersistentState::new("PdmRemoteListCache"),
            _remote_list_observer,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Select(key) => {
                self.active = key;
                true
            }
            Msg::RemoteListChanged(remote_list) => {
                let remote_list_cache: Vec<RemoteListCacheEntry> = remote_list
                    .into_iter()
                    .map(|item| RemoteListCacheEntry {
                        id: item.id.clone(),
                        ty: item.ty,
                    })
                    .collect();

                if *self.remote_list_cache != remote_list_cache {
                    self.remote_list_cache.update(remote_list_cache);
                    true
                } else {
                    false
                }
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let scope = ctx.link().clone();
        let props = ctx.props();

        let mut content = SelectionView::new()
            .class(FlexFit)
            .selection(self.menu_selection.clone());

        let mut menu = Menu::new();

        register_view(
            &mut menu,
            &mut content,
            tr!("Dashboard"),
            "dashboard",
            Some("fa fa-tachometer"),
            move |_| Dashboard::new().into(),
        );

        register_view(
            &mut menu,
            &mut content,
            tr!("Notes"),
            "notes",
            Some("fa fa-sticky-note-o"),
            move |_| {
                let notes = NotesView::new("/config/notes").on_submit(|notes| async move {
                    proxmox_yew_comp::http_put("/config/notes", Some(serde_json::to_value(&notes)?))
                        .await
                });

                Container::new()
                    .class("pwt-content-spacer")
                    .class(pwt::css::FlexFit)
                    .with_child(notes)
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
            |_| html! {<AccessControl/>},
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

        let username = ctx.props().username.clone();
        register_submenu(
            &mut menu,
            &mut content,
            tr!("Administration"),
            "administration",
            Some("fa fa-wrench"),
            move |_| {
                ServerAdministration::new()
                    .username(username.clone())
                    .into()
            },
            admin_submenu,
        );

        let mut remote_submenu = Menu::new();

        for remote in self.remote_list_cache.iter() {
            register_view(
                &mut remote_submenu,
                &mut content,
                &remote.id,
                &format!("remote-{}", remote.id),
                Some("fa fa-server"),
                {
                    let remote = remote.clone();
                    move |_| match remote.ty {
                        RemoteType::Pve => crate::pve::PveRemote::new(remote.id.clone()).into(),
                        RemoteType::Pbs => crate::pbs::DatastoreMenu::new(remote.id.clone()).into(),
                    }
                },
            );
        }

        register_submenu(
            &mut menu,
            &mut content,
            tr!("Remotes"),
            "remotes",
            Some(if props.remote_list_loading {
                "fa fa-fw pwt-loading-icon"
            } else {
                "fa fa-server"
            }),
            |_| {
                Container::new()
                    .class("pwt-content-spacer")
                    .class(pwt::css::FlexFit)
                    .with_child(RemoteConfigPanel::new())
                    .into()
            },
            remote_submenu,
        );

        let drawer = NavigationDrawer::new(menu)
            .aria_label("Datacenter Manager")
            .class("pwt-border-end")
            .class(css::Flex::None)
            .width(275)
            .router(true)
            .default_active(self.active.to_string())
            .selection(self.menu_selection.clone())
            .on_select(Callback::from(move |id: Option<Key>| {
                let id = id.unwrap_or_else(|| Key::from(""));
                scope.send_message(Msg::Select(id))
            }));

        Container::new()
            .class(Display::Flex)
            .class(FlexFit)
            .with_child(
                Row::new()
                    .class(FlexFit)
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
