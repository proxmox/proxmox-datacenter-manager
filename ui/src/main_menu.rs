use std::rc::Rc;

use html::IntoPropValue;
use yew::virtual_dom::{Key, VComp, VNode};

use pwt::css::{self, Display, FlexFit};
use pwt::prelude::*;
use pwt::state::Selection;
use pwt::widget::nav::{Menu, MenuItem, NavigationDrawer};
use pwt::widget::{Container, Row, SelectionView, SelectionViewRenderInfo};

use proxmox_yew_comp::{AclContext, NotesView, XTermJs};

use pdm_api_types::remotes::RemoteType;
use pdm_api_types::{PRIV_SYS_AUDIT, PRIV_SYS_MODIFY};

use crate::dashboard::view::View;
use crate::remotes::RemotesPanel;
use crate::sdn::evpn::EvpnPanel;
use crate::sdn::ZoneTree;
use crate::{
    AccessControl, CertificatesPanel, RemoteListCacheEntry, ServerAdministration,
    SystemConfiguration,
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

    /// Set the list of remotes
    #[builder]
    #[prop_or_default]
    pub remote_list: Vec<RemoteListCacheEntry>,
}

impl MainMenu {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

pub enum Msg {
    Select(Key),
    UpdateAcl(AclContext),
}

pub struct PdmMainMenu {
    active: Key,
    menu_selection: Selection,
    acl_context: AclContext,
    _acl_context_listener: ContextHandle<AclContext>,
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
        let (acl_context, acl_context_listener) = ctx
            .link()
            .context(ctx.link().callback(Msg::UpdateAcl))
            .expect("acl context not present");

        Self {
            active: Key::from("dashboard"),
            menu_selection: Selection::new(),
            acl_context,
            _acl_context_listener: acl_context_listener,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Select(key) => {
                self.active = key;
                true
            }
            Msg::UpdateAcl(acl_context) => {
                self.acl_context = acl_context;
                true
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
            move |_| View::new(None).into(),
        );

        if self.acl_context.check_privs(&["system"], PRIV_SYS_AUDIT) {
            let allow_editing = self
                .acl_context
                .check_privs(&["system", "notes"], PRIV_SYS_MODIFY);

            register_view(
                &mut menu,
                &mut content,
                tr!("Notes"),
                "notes",
                Some("fa fa-sticky-note-o"),
                move |_| {
                    let mut notes = NotesView::new("/config/notes");

                    if allow_editing {
                        notes.set_on_submit(|notes| async move {
                            proxmox_yew_comp::http_put(
                                "/config/notes",
                                Some(serde_json::to_value(&notes)?),
                            )
                            .await
                        });
                    }

                    Container::new()
                        .class("pwt-content-spacer")
                        .class(pwt::css::FlexFit)
                        .with_child(notes)
                        .into()
                },
            )
        }

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

        let mut sdn_submenu = Menu::new();

        register_view(
            &mut sdn_submenu,
            &mut content,
            tr!("EVPN"),
            "evpn",
            Some("fa fa-sitemap"),
            |_| EvpnPanel::new().into(),
        );

        register_submenu(
            &mut menu,
            &mut content,
            tr!("SDN"),
            "sdn",
            Some("fa fa-sdn"),
            |_| ZoneTree::new().into(),
            sdn_submenu,
        );

        let mut remote_submenu = Menu::new();

        for remote in props.remote_list.iter() {
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
                        RemoteType::Pbs => crate::pbs::PbsRemote::new(remote.id.clone()).into(),
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
                "fa fa-refresh fa-spin"
            } else {
                "fa fa-server"
            }),
            |_| {
                Container::new()
                    .class("pwt-content-spacer")
                    .class(pwt::css::FlexFit)
                    .with_child(html! {<RemotesPanel/>})
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

impl From<MainMenu> for VNode {
    fn from(val: MainMenu) -> Self {
        let comp = VComp::new::<PdmMainMenu>(Rc::new(val), None);
        VNode::from(comp)
    }
}
