use anyhow::Error;
use gloo_timers::callback::Timeout;
//use pbs::utils::init_task_descr_table_pbs;
//use pbs_api_types::NodeStatus;
use yew::prelude::*;

use pwt::prelude::*;
use pwt::state::{Loader, PersistentState};
use pwt::widget::{Column, DesktopApp, Dialog, Mask};

use proxmox_login::Authentication;
use proxmox_yew_comp::common_api_types::TaskListItem;
use proxmox_yew_comp::{
    authentication_from_cookie, http_get, register_auth_observer, AuthObserver, LoginPanel,
    SubscriptionAlert,
};

//use pbs::MainMenu;
use pdm::{MainMenu, RemoteList, TopNavBar};
use pdm_api_types::subscription::{RemoteSubscriptionState, RemoteSubscriptions};

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
    remote_list: PersistentState<RemoteList>,
    remote_list_error: Option<String>,
    remote_list_timeout: Option<Timeout>,
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
        match result {
            Err(err) => {
                self.remote_list_error = Some(err.to_string());
                // do not touch remote_list data
                true
            }
            Ok(list) => {
                self.remote_list_error = None;
                self.remote_list.update(list);
                true
            }
        }
    }

    fn poll_remote_list(ctx: &Context<Self>, first: bool) -> Option<Timeout> {
        let link = ctx.link().clone();
        let timeout = Timeout::new(if first { 0 } else { 5_000 }, move || {
            link.send_future(async move { Msg::RemoteList(Self::get_remote_list().await) })
        });
        Some(timeout)
    }

    async fn get_remote_list() -> Result<RemoteList, Error> {
        let mut list = pdm::pdm_client().list_remotes().await?;
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
            .loader("/nodes/localhost/tasks?running=1&limit=100");

        let login_info = authentication_from_cookie(&proxmox_yew_comp::ExistingProduct::PDM);

        let mut this = Self {
            _auth_observer,
            login_info,
            subscription_confirmed: false,
            show_subscription_alert: None,
            running_tasks,
            running_tasks_timeout: None,
            remote_list: PersistentState::new("PdmRemoteList"),
            remote_list_error: None,
            remote_list_timeout: None,
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
                self.show_subscription_alert = Some(true);
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
        let on_login = ctx.link().callback(|info| Msg::Login(info));
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
                    MainMenu::new().username(username.clone()).into()
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

        DesktopApp::new(
            html! {<ContextProvider<RemoteList> {context}>{body}</ContextProvider<RemoteList>>},
        )
        .into()
    }
}

fn main() {
    //init_task_descr_table_pbs();
    proxmox_yew_comp::http_setup(&proxmox_yew_comp::ExistingProduct::PDM);

    pwt::props::set_http_get_method(
        |url| async move { proxmox_yew_comp::http_get(&url, None).await },
    );

    pwt::state::set_available_themes(&["Crisp", "Desktop", "Material"]);
    wasm_logger::init(wasm_logger::Config::default());

    pwt::state::set_available_languages(proxmox_yew_comp::available_language_list());

    yew::Renderer::<DatacenterManagerApp>::new().render();
}
