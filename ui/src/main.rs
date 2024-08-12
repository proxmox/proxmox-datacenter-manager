use gloo_timers::callback::Timeout;
//use pbs::utils::init_task_descr_table_pbs;
//use pbs_api_types::NodeStatus;
use yew::prelude::*;

use pwt::prelude::*;
use pwt::state::Loader;
use pwt::widget::{Column, DesktopApp, Dialog};

use proxmox_login::Authentication;
use proxmox_yew_comp::common_api_types::TaskListItem;
use proxmox_yew_comp::{
    authentication_from_cookie, http_get, register_auth_observer, AuthObserver, LoginPanel,
    SubscriptionAlert,
};

//use pbs::MainMenu;
use pdm::{MainMenu, TopNavBar};

enum Msg {
    ConfirmSubscription,
    ShowSubscriptionAlert(String, Option<String>),
    Login(Authentication),
    // SaveFingerprint(String), FIXME
    Logout,
    TaskChanged,
}

struct DatacenterManagerApp {
    _auth_observer: AuthObserver,
    login_info: Option<Authentication>,
    subscription_confirmed: bool,
    show_subscription_alert: Option<(String, Option<String>)>,
    running_tasks: Loader<Vec<TaskListItem>>,
    running_tasks_timeout: Option<Timeout>,
}

async fn check_subscription() -> Msg {
    let data: Result<serde_json::Value, _> = http_get("/nodes/localhost/subscription", None).await;

    let (status, url) = match &data {
        Ok(data) => (
            data["status"].as_str().unwrap_or("unknown").to_string(),
            data["url"].as_str().map(|s| s.to_string()),
        ),
        Err(_) => ("unknown".into(), None),
    };

    if status == "new" || status == "active" {
        Msg::ConfirmSubscription
    } else {
        Msg::ShowSubscriptionAlert(status, url)
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
    fn on_login(&self, ctx: &Context<Self>, fresh_login: bool) {
        if let Some(info) = &self.login_info {
            self.running_tasks.load();
            if fresh_login {
                ctx.link().send_future(check_subscription());
            } else {
                proxmox_yew_comp::http_set_auth(info.clone());
            }
            //ctx.link().send_future_batch(get_fingerprint());
        }
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

        let this = Self {
            _auth_observer,
            login_info,
            subscription_confirmed: false,
            show_subscription_alert: None,
            running_tasks,
            running_tasks_timeout: None,
        };

        this.on_login(ctx, false);
        this
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::ConfirmSubscription => {
                self.subscription_confirmed = true;
                self.show_subscription_alert = None;
                true
            }
            Msg::ShowSubscriptionAlert(status, url) => {
                self.show_subscription_alert = Some((status, url));
                true
            }
            Msg::Logout => {
                //log::info!("CLEAR COOKIE");
                proxmox_yew_comp::http_clear_auth();
                self.login_info = None;
                self.running_tasks_timeout = None;
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
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let on_login = ctx.link().callback(|info| Msg::Login(info));
        let subscription_alert = self.show_subscription_alert.as_ref().map(|(status, url)| {
            SubscriptionAlert::new(status.to_string())
                .url(url.clone().map(|s| s.to_string()))
                .on_close(ctx.link().callback(|_| Msg::ConfirmSubscription))
        });

        let body = Column::new()
            .class("pwt-viewport")
            .with_child(
                TopNavBar::new(self.running_tasks.clone())
                    .username(self.login_info.as_ref().map(|info| info.userid.to_owned()))
                    .on_logout(ctx.link().callback(|_| Msg::Logout)),
            )
            .with_child({
                let main_view: Html = if self.login_info.is_some() {
                    MainMenu::new(self.running_tasks.clone()).into()
                } else {
                    Dialog::new(tr!("Proxmox Datacenter Manager Login"))
                        .with_child(LoginPanel::new().on_login(on_login))
                        .into()
                };
                main_view
            })
            .with_optional_child(subscription_alert);

        DesktopApp::new(body).into()
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
