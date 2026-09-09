use std::rc::Rc;

use anyhow::{Error, format_err};
use yew::html::IntoPropValue;
use yew::prelude::*;

use pwt::AsyncAbortGuard;
use pwt::prelude::*;
use pwt::props::RenderFn;
use pwt::state::Store;
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::{Selector, SelectorRenderArgs, ValidateFn};
use pwt::widget::{Button, Column, GridPicker, Row};

use proxmox_yew_comp::common_api_types::BasicRealmInfo;
use proxmox_yew_comp::percent_encoding::percent_encode_component;

use pwt::props::{FieldBuilder, WidgetBuilder};
use pwt_macros::{builder, widget};

use pdm_api_types::remotes::RemoteType;

use crate::remotes::connection_attempt::{ConnectionAttempt, attempt_mask, failure_notice};

#[widget(comp=RemoteRealmSelectorComp, @input)]
#[derive(Clone, Properties, PartialEq)]
#[builder]
pub struct RemoteRealmSelector {
    /// The default value.
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub default: Option<AttrValue>,

    /// Hostname/IP of the remote that should be queried for its realms.
    pub hostname: AttrValue,

    /// Optional fingerprint of the remote.
    pub fingerprint: Option<AttrValue>,

    /// Type of the remote.
    pub remote_type: RemoteType,

    /// Whether the wizard page containing this selector is active.
    #[builder]
    #[prop_or_default]
    pub active: bool,
}

impl RemoteRealmSelector {
    /// Create a new instance of this component.
    pub fn new(
        hostname: impl IntoPropValue<AttrValue>,
        fingerprint: impl IntoPropValue<Option<AttrValue>>,
        remote_type: RemoteType,
    ) -> Self {
        yew::props!(Self {
            hostname: hostname.into_prop_value(),
            fingerprint: fingerprint.into_prop_value(),
            remote_type,
        })
    }
}

enum Msg {
    Load,
    LoadResult {
        load_id: usize,
        result: Result<Vec<BasicRealmInfo>, Error>,
    },
    Cancel,
    Tick,
    Redraw,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RealmLoadStatus {
    Idle,
    Loading,
    Canceled,
    Loaded,
    Failed,
}

#[derive(Debug, Eq, PartialEq)]
struct RealmLoadState {
    load_id: usize,
    status: RealmLoadStatus,
}

impl Default for RealmLoadState {
    fn default() -> Self {
        Self {
            load_id: 0,
            status: RealmLoadStatus::Idle,
        }
    }
}

impl RealmLoadState {
    fn start(&mut self) -> usize {
        self.load_id = self.load_id.wrapping_add(1);
        self.status = RealmLoadStatus::Loading;
        self.load_id
    }

    fn cancel(&mut self) -> bool {
        if !self.is_loading() {
            return false;
        }

        self.reset_to(RealmLoadStatus::Canceled);
        true
    }

    fn reset(&mut self) {
        self.reset_to(RealmLoadStatus::Idle);
    }

    fn finish(&mut self, load_id: usize, loaded: bool) -> bool {
        if load_id != self.load_id || !self.is_loading() {
            return false;
        }

        self.status = if loaded {
            RealmLoadStatus::Loaded
        } else {
            RealmLoadStatus::Failed
        };
        true
    }

    fn reset_to(&mut self, status: RealmLoadStatus) {
        self.load_id = self.load_id.wrapping_add(1);
        self.status = status;
    }

    fn is_loaded(&self) -> bool {
        self.status == RealmLoadStatus::Loaded
    }

    fn is_loading(&self) -> bool {
        self.status == RealmLoadStatus::Loading
    }

    /// A canceled load leaves no error to report, so it needs its own way back to a realm list.
    fn is_canceled(&self) -> bool {
        self.status == RealmLoadStatus::Canceled
    }
}

struct RemoteRealmSelectorComp {
    store: Store<BasicRealmInfo>,
    validate: ValidateFn<(String, Store<BasicRealmInfo>)>,
    picker: RenderFn<SelectorRenderArgs<Store<BasicRealmInfo>>>,
    load_state: RealmLoadState,
    last_error: Option<Error>,
    load_guard: Option<AsyncAbortGuard>,
    attempt: Option<ConnectionAttempt>,
}

impl RemoteRealmSelectorComp {
    fn should_load(props: &RemoteRealmSelector) -> bool {
        props.active && !props.is_disabled() && !props.hostname.is_empty()
    }

    fn cancel_load(&mut self) {
        if self.load_state.cancel() {
            self.load_guard = None;
            self.attempt = None;
        }
    }

    fn reset_load(&mut self) {
        self.load_state.reset();
        self.load_guard = None;
        self.attempt = None;
        self.last_error = None;
    }

    fn start_load(&mut self, ctx: &Context<Self>) {
        if !Self::should_load(ctx.props()) {
            self.reset_load();
            return;
        }

        let load_id = self.load_state.start();
        self.load_guard = None;
        self.attempt = None;
        self.last_error = None;
        let link = ctx.link().clone();
        self.attempt = Some(ConnectionAttempt::new(link.callback(|_| Msg::Tick)));
        let url = realm_url(
            &ctx.props().hostname,
            ctx.props().fingerprint.as_deref(),
            ctx.props().remote_type,
        );
        self.load_guard = Some(AsyncAbortGuard::spawn(async move {
            let result = proxmox_yew_comp::http_get(&url, None).await;
            link.send_message(Msg::LoadResult { load_id, result });
        }));
    }
}

impl Component for RemoteRealmSelectorComp {
    type Message = Msg;
    type Properties = RemoteRealmSelector;

    fn create(ctx: &Context<Self>) -> Self {
        let store = Store::new().on_change(ctx.link().callback(|_| Msg::Redraw));

        let validate = ValidateFn::new(|(realm, store): &(String, Store<BasicRealmInfo>)| {
            if store.read().data().iter().any(|item| &item.realm == realm) {
                Ok(())
            } else {
                Err(format_err!("no such realm"))
            }
        });

        let picker = RenderFn::new({
            let columns = columns();
            move |args: &SelectorRenderArgs<Store<BasicRealmInfo>>| {
                let table = DataTable::new(columns.clone(), args.store.clone()).class("pwt-fit");

                GridPicker::new(table)
                    .selection(args.selection.clone())
                    .on_select(args.controller.on_select_callback())
                    .into()
            }
        });

        if Self::should_load(ctx.props()) {
            ctx.link().send_message(Msg::Load);
        }

        Self {
            store,
            validate,
            picker,
            load_state: RealmLoadState::default(),
            last_error: None,
            load_guard: None,
            attempt: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Load => {
                self.start_load(ctx);
            }
            Msg::LoadResult { load_id, result } => {
                if !self.load_state.finish(load_id, result.is_ok()) {
                    return false;
                }
                self.load_guard = None;
                self.attempt = None;
                match result {
                    Ok(data) => {
                        self.last_error = None;
                        self.store.set_data(data);
                    }
                    Err(error) => {
                        self.last_error = Some(error);
                    }
                }
            }
            Msg::Cancel => {
                self.cancel_load();
            }
            Msg::Tick => {
                return self.attempt.as_mut().is_some_and(ConnectionAttempt::tick);
            }
            Msg::Redraw => {}
        }
        true
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();
        let source_changed = props.hostname != old_props.hostname
            || props.fingerprint != old_props.fingerprint
            || props.remote_type != old_props.remote_type;

        if source_changed {
            self.reset_load();
            self.store.clear();
            self.last_error = None;
            if Self::should_load(props) {
                ctx.link().send_message(Msg::Load);
            }
        } else if !Self::should_load(props) {
            if !self.load_state.is_loaded() {
                self.reset_load();
                self.last_error = None;
            }
        } else if (!old_props.active || old_props.is_disabled()) && !self.load_state.is_loaded() {
            ctx.link().send_message(Msg::Load);
        }

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let selector = Selector::new(self.store.clone(), self.picker.clone())
            .with_std_props(&props.std_props)
            .with_input_props(&props.input_props)
            .disabled(props.is_disabled())
            .default(props.default.as_deref().unwrap_or("pam").to_string())
            .validate(self.validate.clone());

        let mask = attempt_mask(
            selector,
            self.attempt.as_ref(),
            &tr!("Loading realms"),
            ctx.link().callback(|_| Msg::Cancel),
        );
        let notice = self.last_error.as_ref().map(|error| {
            failure_notice(
                error,
                &props.hostname,
                Some(ctx.link().callback(|_| Msg::Load)),
            )
        });

        Column::new()
            .gap(1)
            .with_child(mask)
            .with_optional_child(notice)
            .with_optional_child(self.load_state.is_canceled().then(|| {
                Row::new().with_child(
                    Button::new(tr!("Reload Realms"))
                        .on_activate(ctx.link().callback(|_| Msg::Load)),
                )
            }))
            .into()
    }
}

fn realm_url(hostname: &str, fingerprint: Option<&str>, remote_type: RemoteType) -> String {
    let mut url = format!(
        "/{remote_type}/realms?hostname={}",
        percent_encode_component(hostname),
    );
    if let Some(fingerprint) = fingerprint {
        url.push_str(&format!(
            "&fingerprint={}",
            percent_encode_component(fingerprint)
        ));
    }
    url
}

fn columns() -> Rc<Vec<DataTableHeader<BasicRealmInfo>>> {
    Rc::new(vec![
        DataTableColumn::new("Realm")
            .width("100px")
            .sort_order(true)
            .show_menu(false)
            .get_property(|record: &BasicRealmInfo| &record.realm)
            .into(),
        DataTableColumn::new("Comment")
            .width("300px")
            .show_menu(false)
            .get_property_owned(|record: &BasicRealmInfo| {
                record.comment.clone().unwrap_or_default()
            })
            .into(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_canceled_load_offers_its_own_way_back() {
        let mut state = RealmLoadState::default();
        state.start();
        assert!(state.cancel());
        assert!(state.is_canceled());

        let load_id = state.start();
        assert!(!state.is_canceled());
        assert!(state.finish(load_id, true));
        assert!(state.is_loaded());
    }

    #[test]
    fn stale_load_results_do_not_replace_the_active_attempt() {
        let mut state = RealmLoadState::default();
        let stale_id = state.start();
        assert!(state.cancel());
        let active_id = state.start();

        assert!(!state.finish(stale_id, true));
        assert_eq!(state.status, RealmLoadStatus::Loading);
        assert!(state.finish(active_id, true));
        assert!(state.is_loaded());
    }

    #[test]
    fn reset_clears_the_canceled_and_failed_states() {
        let mut state = RealmLoadState::default();
        state.start();
        assert!(state.cancel());
        state.reset();
        assert!(!state.is_canceled());

        let load_id = state.start();
        assert!(state.finish(load_id, false));
        assert_eq!(state.status, RealmLoadStatus::Failed);
        state.reset();
        assert_eq!(state.status, RealmLoadStatus::Idle);
    }

    #[test]
    fn realm_url_contains_the_connection_parameters() {
        assert_eq!(
            realm_url("node.example.test:8006", Some("AA:BB"), RemoteType::Pve),
            "/pve/realms?hostname=node%2Eexample%2Etest%3A8006&fingerprint=AA%3ABB"
        );
    }
}
