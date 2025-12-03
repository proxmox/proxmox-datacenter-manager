use std::rc::Rc;

use anyhow::Error;
use pwt::css::ColorScheme;
use serde::Deserialize;
use wasm_bindgen::UnwrapThrowExt;

use pwt::prelude::*;
use pwt::widget::menu::{Menu, MenuButton, MenuEntry, MenuEvent, MenuItem};
use pwt::AsyncAbortGuard;
use yew::html::{IntoEventCallback, IntoPropValue};
use yew::virtual_dom::{VComp, VNode};

use pwt::state::{Loader, Theme, ThemeObserver};
use pwt::widget::{Button, Container, Row, ThemeModeSelector, Tooltip};

use proxmox_yew_comp::utils::set_location_href;
use proxmox_yew_comp::RunningTasksButton;
use proxmox_yew_comp::{http_get, LanguageDialog, TaskViewer, ThemeDialog};

use pwt_macros::builder;

use pbs_api_types::TaskListItem;
use pdm_api_types::RemoteUpid;

use crate::tasks::format_optional_remote_upid;
use crate::widget::SearchBox;

#[derive(Deserialize)]
pub struct VersionInfo {
    version: String,
    release: String,
    // repoid: String,
}

async fn load_version() -> Result<VersionInfo, Error> {
    http_get("/version", None).await
}

#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct TopNavBar {
    running_tasks: Loader<Vec<TaskListItem>>,

    #[builder_cb(IntoEventCallback, into_event_callback, ())]
    #[prop_or_default]
    pub on_logout: Option<Callback<()>>,
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub username: Option<String>,
}

impl TopNavBar {
    pub fn new(running_tasks: Loader<Vec<TaskListItem>>) -> Self {
        yew::props!(Self { running_tasks })
    }
}

#[derive(Clone)]
pub enum ViewState {
    LanguageDialog,
    ThemeDialog,
    OpenTask((String, Option<i64>)),
}

pub enum Msg {
    ThemeChanged((Theme, /* dark_mode */ bool)),
    Load,
    LoadResult(Result<VersionInfo, Error>),
    ChangeView(Option<ViewState>),
}

pub struct PdmTopNavBar {
    _theme_observer: ThemeObserver,
    dark_mode: bool,
    version_info: Option<VersionInfo>,
    view_state: Option<ViewState>,
    abort_guard: Option<AsyncAbortGuard>,
}

impl Component for PdmTopNavBar {
    type Message = Msg;
    type Properties = TopNavBar;

    fn create(ctx: &Context<Self>) -> Self {
        let props = ctx.props();

        let _theme_observer = ThemeObserver::new(ctx.link().callback(Msg::ThemeChanged));
        let dark_mode = _theme_observer.dark_mode();

        if props.username.is_some() {
            ctx.link().send_message(Msg::Load);
        }

        Self {
            _theme_observer,
            dark_mode,
            version_info: None,
            view_state: None,
            abort_guard: None,
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();
        if props.username != old_props.username {
            if props.username.is_some() {
                ctx.link().send_message(Msg::Load);
            } else {
                self.version_info = None;
            }
        }
        true
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::ChangeView(view_state) => {
                self.view_state = view_state;
                true
            }
            Msg::ThemeChanged((_theme, dark_mode)) => {
                self.dark_mode = dark_mode;
                true
            }
            Msg::Load => {
                let link = ctx.link().clone();
                self.abort_guard.replace(AsyncAbortGuard::spawn(async move {
                    link.send_message(Msg::LoadResult(load_version().await))
                }));
                true
            }
            Msg::LoadResult(result) => {
                self.version_info = match result {
                    Ok(version_info) => Some(version_info),
                    Err(err) => {
                        log::error!("load version info failed: {}", err);
                        None
                    }
                };
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();

        let on_logout = props
            .on_logout
            .clone()
            .map(|cb| Callback::from(move |_event: MenuEvent| cb.emit(())));

        let menu = Menu::new()
            // FIXME: implement
            //.with_item(MenuItem::new(tr!("My Settings")).icon_class("fa fa-gear"))
            .with_item(
                MenuItem::new(tr!("Language"))
                    .icon_class("fa fa-language")
                    .on_select(
                        ctx.link()
                            .callback(|_| Msg::ChangeView(Some(ViewState::LanguageDialog))),
                    ),
            )
            .with_item(
                MenuItem::new(tr!("Theme"))
                    .icon_class("fa fa-desktop")
                    .on_select(
                        ctx.link()
                            .callback(|_| Msg::ChangeView(Some(ViewState::ThemeDialog))),
                    ),
            )
            .with_item(MenuEntry::Separator)
            .with_item(
                MenuItem::new(tr!("Logout"))
                    .icon_class("fa fa-sign-out")
                    .on_select(on_logout),
            );

        let mut button_group = Row::new()
            .class("pwt-align-items-center")
            .gap(2)
            .with_child(ThemeModeSelector::new().class("pwt-scheme-neutral-alt"))
            // FIXME: implement
            //.with_child(HelpButton::new().class("pwt-scheme-neutral"))
            .with_child(
                Tooltip::new(
                    Button::new(tr!("Documentation"))
                        .icon_class("fa fa-book")
                        .class(ColorScheme::Neutral)
                        .on_activate(|_| {
                            gloo_utils::window()
                                .open_with_url_and_target("docs/index.html", "_blank")
                                .expect_throw("could not open documentation in a new window");
                        }),
                )
                .tip(tr!("Open the Beta documentation in a new tab.")),
            );

        if let Some(username) = &props.username {
            button_group.add_child(
                RunningTasksButton::new(props.running_tasks.clone())
                    .on_show_task(
                        ctx.link()
                            .callback(|info| Msg::ChangeView(Some(ViewState::OpenTask(info)))),
                    )
                    .buttons(vec![
                        Button::new(tr!("Show Local Tasks"))
                            .class(ColorScheme::Primary)
                            .on_activate(move |_| {
                                set_location_href("#/administration/tasks");
                            }),
                        Button::new(tr!("Show Remote Tasks"))
                            .class(ColorScheme::Primary)
                            .on_activate(move |_| {
                                set_location_href("#/remotes/tasks");
                            }),
                    ])
                    .render(|item: &TaskListItem| {
                        format_optional_remote_upid(&item.upid, true).into()
                    }),
            );

            button_group.add_child(
                MenuButton::new(username.clone())
                    .icon_class("fa fa-user")
                    .show_arrow(true)
                    .class(ColorScheme::Tertiary)
                    .menu(menu),
            );
        }

        let dialog: Option<Html> = self.view_state.as_ref().map(|view_state| match view_state {
            ViewState::LanguageDialog => LanguageDialog::new()
                .on_close(ctx.link().callback(|_| Msg::ChangeView(None)))
                .into(),
            ViewState::ThemeDialog => ThemeDialog::new()
                .on_close(ctx.link().callback(|_| Msg::ChangeView(None)))
                .into(),
            ViewState::OpenTask((task_id, _endtime)) => {
                let base_url = task_id
                    .parse::<RemoteUpid>()
                    .ok()
                    .map(|upid| format!("/{}/remotes/{}/tasks", upid.remote_type(), upid.remote()));
                TaskViewer::new(task_id)
                    .base_url(base_url.unwrap_or("/nodes/localhost/tasks".to_string()))
                    .on_close(ctx.link().callback(|_| Msg::ChangeView(None)))
                    .into()
            }
        });

        let src = if self.dark_mode {
            "/images/proxmox_logo_white.svg"
        } else {
            "/images/proxmox_logo.svg"
        };

        Row::new()
            .attribute("role", "banner")
            .attribute("aria-label", "Datacenter Manager")
            //.class("pwt-bg-color-tertiary-container")
            .class("pwt-bg-color-neutral-alt")
            .class("pwt-justify-content-space-between pwt-align-items-center")
            .class("pwt-border-bottom")
            .padding(2)
            .with_child(html! {
                <a href="https://www.proxmox.com" target="_blank">
                    <img {src} height="30" alt="Proxmox logo"/>
                </a>
            })
            .with_child({
                let text = if let Some(info) = &self.version_info {
                    format!("Datacenter Manager {}.{}", info.version, info.release)
                } else {
                    "Datacenter Manager".into()
                };
                Container::from_tag("span")
                    .class("pwt-font-title-medium")
                    .padding_x(4)
                    .with_child(text)
            })
            .with_flex_spacer()
            .with_child(SearchBox::new())
            .with_flex_spacer()
            .with_child(button_group)
            .with_optional_child(dialog)
            .into()
    }
}

impl From<TopNavBar> for VNode {
    fn from(val: TopNavBar) -> Self {
        let comp = VComp::new::<PdmTopNavBar>(Rc::new(val), None);
        VNode::from(comp)
    }
}
