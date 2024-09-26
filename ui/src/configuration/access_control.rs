use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;
use yew::virtual_dom::{Key, VComp, VNode};
use yew::{html, Html, Properties};

use pwt::state::{Selection, Store};
use pwt::tr;
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};

use proxmox_yew_comp::{LoadableComponent, LoadableComponentContext, LoadableComponentMaster};

use pdm_client::types::UserWithTokens;

#[derive(Default, PartialEq, Properties)]
pub struct AccessControl;

impl AccessControl {
    pub fn new() -> Self {
        Self
    }
}

impl From<AccessControl> for VNode {
    fn from(this: AccessControl) -> VNode {
        VComp::new::<LoadableComponentMaster<AccessControlPanel>>(Rc::new(this), None).into()
    }
}

struct AccessControlPanel {
    store: Store<UserWithTokens>,
    selection: Selection,
    columns: Rc<Vec<DataTableHeader<UserWithTokens>>>,
}

enum Msg {
    SelectionChange,
}

#[derive(PartialEq)]
enum ViewState {}

impl LoadableComponent for AccessControlPanel {
    type Message = Msg;
    type Properties = AccessControl;
    type ViewState = ViewState;

    fn load(
        &self,
        _ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        let store = self.store.clone();
        Box::pin(async move {
            let data = crate::pdm_client().list_users(false).await?;
            store.write().set_data(data);
            Ok(())
        })
    }

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        let store = Store::with_extract_key(|record: &UserWithTokens| {
            Key::from(record.user.userid.to_string())
        });

        let selection = Selection::new().on_select(ctx.link().callback(|_| Msg::SelectionChange));

        Self {
            store,
            selection,
            columns: columns(),
        }
    }

    fn update(&mut self, _ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::SelectionChange => true,
        }
    }

    fn toolbar(&self, _ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        None
    }

    fn main_view(&self, _ctx: &LoadableComponentContext<Self>) -> Html {
        DataTable::new(Rc::clone(&self.columns), self.store.clone())
            .class("pwt-flex-fit")
            .selection(self.selection.clone())
            //.on_row_dblclick(move |_: &mut _| {
            //    link.change_view(Some(ViewState::Edit));
            //})
            .into()
    }

    fn dialog_view(
        &self,
        _ctx: &LoadableComponentContext<Self>,
        _view_state: &Self::ViewState,
    ) -> Option<Html> {
        None
    }
}

fn columns() -> Rc<Vec<DataTableHeader<UserWithTokens>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("User name"))
            .width("200px")
            .render(|item: &UserWithTokens| {
                html! {
                    item.user.userid.name().as_str()
                }
            })
            .sorter(|a: &UserWithTokens, b: &UserWithTokens| {
                a.user
                    .userid
                    .name()
                    .as_str()
                    .cmp(b.user.userid.name().as_str())
            })
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Realm"))
            .width("200px")
            .render(|item: &UserWithTokens| {
                html! {
                    item.user.userid.realm().as_str()
                }
            })
            .sorter(|a: &UserWithTokens, b: &UserWithTokens| {
                a.user
                    .userid
                    .realm()
                    .as_str()
                    .cmp(b.user.userid.realm().as_str())
            })
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Enabled"))
            .width("200px")
            .render(|item: &UserWithTokens| {
                html! {
                    match item.user.enable.unwrap_or(true) {
                        true => tr!("Yes"),
                        false => tr!("No"),
                    }
                }
            })
            .sorter(|a: &UserWithTokens, b: &UserWithTokens| a.user.enable.cmp(&b.user.enable))
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Expire"))
            .width("200px")
            .render(|_item: &UserWithTokens| {
                html! { "TODO: date" }
            })
            .sorter(|a: &UserWithTokens, b: &UserWithTokens| a.user.expire.cmp(&b.user.expire))
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Name"))
            .width("200px")
            .render(|item: &UserWithTokens| {
                html! {
                    match (item.user.firstname.as_deref(), item.user.lastname.as_deref()) {
                        (None, None) => String::new(),
                        (Some(f), None) => f.to_string(),
                        (Some(f), Some(l)) => format!("{f} {l}"),
                        (None, Some(l)) => l.to_string(),
                    }
                }
            })
            .sorter(|a: &UserWithTokens, b: &UserWithTokens| {
                use std::cmp::Ordering;
                match a.user.lastname.cmp(&b.user.lastname) {
                    Ordering::Equal => a.user.firstname.cmp(&b.user.firstname),
                    o => o,
                }
            })
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("TFA Lock"))
            .width("200px")
            .render(|item: &UserWithTokens| {
                html! { match item.tfa_locked_until {
                    None => tr!("No"),
                    Some(_) => tr!("TODO: time display"),
                }}
            })
            .sorter(|a: &UserWithTokens, b: &UserWithTokens| {
                a.tfa_locked_until.cmp(&b.tfa_locked_until)
            })
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Comment"))
            .render(|item: &UserWithTokens| {
                html! { item.user.comment.as_deref().unwrap_or_default() }
            })
            .sorter(|a: &UserWithTokens, b: &UserWithTokens| a.user.comment.cmp(&b.user.comment))
            .sort_order(true)
            .into(),
    ])
}
