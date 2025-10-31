use anyhow::Error;
use gloo_timers::callback::Timeout;
use serde_json::json;
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;

//use pbs::utils::init_task_descr_table_pbs;
//use pbs_api_types::NodeStatus;
use yew::prelude::*;

use pwt::prelude::*;
use pwt::props::TextRenderFn;
use pwt::state::{Loader, PersistentState};
use pwt::widget::{Column, DesktopApp, Dialog, Mask};

use pbs_api_types::TaskListItem;
use proxmox_login::Authentication;
use proxmox_yew_comp::utils::init_task_descr_table_base;
use proxmox_yew_comp::{
    authentication_from_cookie, http_get, register_auth_observer, AuthObserver, LoginPanel,
    SubscriptionAlert,
};

//use pbs::MainMenu;
use pdm_api_types::subscription::{RemoteSubscriptionState, RemoteSubscriptions};
use pdm_ui::{
    register_pve_tasks, MainMenu, RemoteList, RemoteListCacheEntry, SearchProvider, TopNavBar,
};

type MsgRemoteList = Result<RemoteList, Error>;

enum Msg {
    ConfirmSubscription,
    ShowSubscriptionAlert,
    Login(Authentication),
    // SaveFingerprint(String), FIXME
    Logout,
    TaskChanged,
    RemoteList(MsgRemoteList),
}

struct DatacenterManagerApp {
    _auth_observer: AuthObserver,
    login_info: Option<Authentication>,
    subscription_confirmed: bool,
    show_subscription_alert: Option<bool>,
    running_tasks: Loader<Vec<TaskListItem>>,
    running_tasks_timeout: Option<Timeout>,
    remote_list: RemoteList,
    remote_list_cache: PersistentState<Vec<RemoteListCacheEntry>>,
    remote_list_error: Option<String>,
    remote_list_timeout: Option<Timeout>,
    search_provider: SearchProvider,
}

async fn check_subscription() -> Msg {
    let data: Result<Vec<RemoteSubscriptions>, _> = http_get("/resources/subscription", None).await;

    let show_alert = match data {
        Ok(list) => list
            .into_iter()
            .any(|info| info.state == RemoteSubscriptionState::None),
        Err(_) => false,
    };

    if show_alert {
        Msg::ShowSubscriptionAlert
    } else {
        Msg::ConfirmSubscription
    }
}

/*
async fn get_fingerprint() -> Option<Msg> {
    http_get("/nodes/localhost/status", None)
        .await
        .ok()
        .map(|data: NodeStatus| Msg::SaveFingerprint(data.info.fingerprint))
}
*/
impl DatacenterManagerApp {
    fn on_login(&mut self, ctx: &Context<Self>, fresh_login: bool) {
        if let Some(info) = &self.login_info {
            self.running_tasks.load();
            if fresh_login {
                if self.subscription_confirmed {
                    ctx.link().send_message(Msg::ConfirmSubscription);
                } else {
                    ctx.link().send_future(check_subscription());
                }
            } else {
                ctx.link().send_message(Msg::ConfirmSubscription);
                proxmox_yew_comp::http_set_auth(info.clone());
            }
            //ctx.link().send_future_batch(get_fingerprint());
            //
            self.remote_list_timeout = Self::poll_remote_list(ctx, true);
        }
    }

    fn update_remotes(&mut self, ctx: &Context<Self>, result: MsgRemoteList) -> bool {
        self.remote_list_timeout = Self::poll_remote_list(ctx, false);
        let mut changed = false;
        match result {
            Err(err) => {
                if self.remote_list_error.is_none() {
                    self.remote_list_error = Some(err.to_string());
                    changed = true;
                }
                // do not touch remote_list data
            }
            Ok(list) => {
                if self.remote_list_error.is_some() {
                    self.remote_list_error = None;
                    changed = true;
                }
                if self.remote_list != list {
                    self.remote_list = list.clone();
                    changed = true;
                }

                let remote_list_cache: Vec<RemoteListCacheEntry> = list
                    .iter()
                    .map(|item| RemoteListCacheEntry {
                        id: item.id.clone(),
                        ty: item.ty,
                    })
                    .collect();

                if *self.remote_list_cache != remote_list_cache {
                    self.remote_list_cache.update(remote_list_cache);
                    changed = true;
                }
            }
        }
        changed
    }

    fn poll_remote_list(ctx: &Context<Self>, first: bool) -> Option<Timeout> {
        let link = ctx.link().clone();
        let timeout = Timeout::new(if first { 0 } else { 5_000 }, move || {
            link.send_future(async move { Msg::RemoteList(Self::get_remote_list().await) })
        });
        Some(timeout)
    }

    async fn get_remote_list() -> Result<RemoteList, Error> {
        let mut list = pdm_ui::pdm_client().list_remotes().await?;
        list.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(RemoteList(list))
    }
}

impl Component for DatacenterManagerApp {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let _auth_observer = register_auth_observer(ctx.link().callback(|_| {
            log::info!("AUTH OBSERVER - AUTH FAILED");
            Msg::Logout
        }));

        let running_tasks = Loader::new()
            .on_change(ctx.link().callback(|_| Msg::TaskChanged))
            .loader((
                |url: AttrValue| async move {
                    // TODO replace with pdm client call
                    let params = Some(json!({
                        "limit": 100,
                        "running": true,
                    }));
                    let mut res: Vec<TaskListItem> =
                        http_get(url.to_string(), params.clone()).await?;

                    let res2: Vec<_> = http_get("/remote-tasks/list", params).await?;
                    res.extend_from_slice(&res2);

                    Ok(res.into_iter().take(100).collect())
                },
                "/nodes/localhost/tasks",
            ));

        let login_info = authentication_from_cookie(&proxmox_yew_comp::ExistingProduct::PDM);

        let mut this = Self {
            _auth_observer,
            login_info,
            subscription_confirmed: false,
            show_subscription_alert: None,
            running_tasks,
            running_tasks_timeout: None,
            remote_list: Vec::new().into(),
            remote_list_cache: PersistentState::new("PdmRemoteListCache"),
            remote_list_error: None,
            remote_list_timeout: None,
            search_provider: SearchProvider::new(),
        };

        this.on_login(ctx, false);
        this
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::ConfirmSubscription => {
                self.subscription_confirmed = true;
                self.show_subscription_alert = Some(false);
                true
            }
            Msg::ShowSubscriptionAlert => {
                // Disable for alpha and rework for a beta or stable version to avoid friction if a
                // few unsubscribed test instances are present in another subscribed (big) setup.
                // self.show_subscription_alert = Some(true);
                self.subscription_confirmed = true;
                self.show_subscription_alert = Some(false);
                true
            }
            Msg::Logout => {
                //log::info!("CLEAR COOKIE");
                self.remote_list_timeout = None;
                proxmox_yew_comp::http_clear_auth();
                self.login_info = None;
                self.running_tasks_timeout = None;
                self.show_subscription_alert = None;
                true
            }
            Msg::Login(info) => {
                //log::info!("LOGIN");
                self.login_info = Some(info);
                self.on_login(ctx, true);
                true
            }
            Msg::TaskChanged => {
                let running_tasks = self.running_tasks.clone();
                self.running_tasks_timeout = Some(Timeout::new(3000, move || {
                    running_tasks.load();
                }));
                false
            } /*
            Msg::SaveFingerprint(fp) => {
            PersistentState::<String>::with_location(
            "fingerprint",
            pwt::state::StorageLocation::Session,
            )
            .update(fp);
            false
            }
             */
            Msg::RemoteList(remotes) => self.update_remotes(ctx, remotes),
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let on_login = ctx.link().callback(Msg::Login);
        let loading = self.login_info.is_some() && self.show_subscription_alert.is_none();
        let subscription_alert = self.show_subscription_alert.and_then(|show| {
            (self.login_info.is_some() && show).then_some(
                SubscriptionAlert::new("notfound".to_string())
                    .on_close(ctx.link().callback(|_| Msg::ConfirmSubscription)),
            )
        });

        let username = self.login_info.as_ref().map(|info| info.userid.to_owned());
        let body = Column::new()
            .class("pwt-viewport")
            .with_child(
                TopNavBar::new(self.running_tasks.clone())
                    .username(username.clone())
                    .on_logout(ctx.link().callback(|_| Msg::Logout)),
            )
            .with_child({
                let main_view: Html = if self.login_info.is_some() && !loading {
                    MainMenu::new()
                        .username(username.clone())
                        .remote_list(self.remote_list_cache.clone())
                        .remote_list_loading(self.remote_list_error.is_some())
                        .into()
                } else {
                    Dialog::new(tr!("Proxmox Datacenter Manager Login"))
                        .with_child(
                            Mask::new(LoginPanel::new().on_login(on_login)).visible(loading),
                        )
                        .into()
                };
                main_view
            })
            .with_optional_child(subscription_alert);

        let context = self.remote_list.clone();
        let search_context = self.search_provider.clone();

        DesktopApp::new(html! {
            <ContextProvider<SearchProvider> context={search_context}>
                <ContextProvider<RemoteList> {context}>
                    {body}
                </ContextProvider<RemoteList>>
            </ContextProvider<SearchProvider>>
        })
        .catalog_url_builder(TextRenderFn::new(|lang| {
            format!("locale/catalog-{lang}.mo")
        }))
        .into()
    }
}

fn set_body(content: &str) {
    let document = web_sys::window().unwrap().document().unwrap();
    let body: HtmlElement = document.create_element("body").unwrap().dyn_into().unwrap();
    body.set_inner_html(content);
    document.set_body(Some(&body));
}

fn panic_hook() -> Box<dyn Fn(&std::panic::PanicHookInfo) + 'static + Sync + Send> {
    Box::new(|info: &std::panic::PanicHookInfo<'_>| {
        let msg = format!("Application panicked: {info}");
        web_sys::console::error_1(&msg.into());

        set_body(&format!(
            r#"
<h1 class="panicked__title">Application panicked!</h1>
<p>Reason: {info}</p>
"#
        ));
    })
}

fn main() {
    wasm_logger::init(wasm_logger::Config::default());

    yew::set_custom_panic_hook(panic_hook());

    init_task_descr_table_base();
    register_pve_tasks();
    proxmox_yew_comp::http_setup(&proxmox_yew_comp::ExistingProduct::PDM);

    pwt::props::set_http_get_method(
        |url| async move { proxmox_yew_comp::http_get(&url, None).await },
    );

    pwt::state::set_available_themes(&["Desktop", "Crisp"]);

    pwt::state::set_available_languages(proxmox_yew_comp::available_language_list());

    yew::Renderer::<DatacenterManagerApp>::new().render();
}
