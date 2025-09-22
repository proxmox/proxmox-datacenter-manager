use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;

use proxmox_schema::property_string::PropertyString;

use crate::remotes::edit_remote::EditRemote;
//use pwt::widget::form::{Field, FormContext, InputType};

use pdm_api_types::remotes::Remote;
//use proxmox_schema::{property_string::PropertyString, ApiType};
use proxmox_yew_comp::percent_encoding::percent_encode_component;

//use pbs_api_types::CERT_FINGERPRINT_SHA256_SCHEMA;

//use proxmox_schema::api_types::{CERT_FINGERPRINT_SHA256_SCHEMA, DNS_NAME_OR_IP_SCHEMA};

use serde_json::Value;
use yew::virtual_dom::{Key, VComp, VNode};

use pwt::prelude::*;
use pwt::state::{Selection, Store};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
//use pwt::widget::form::{delete_empty_values, Field, FormContext, InputType};
use pwt::widget::{
    menu::{Menu, MenuButton, MenuItem},
    Button,
    Column,
    Toolbar,
    Tooltip,
};
//use pwt::widget::InputPanel;

//use proxmox_yew_comp::EditWindow;
use proxmox_yew_comp::{
    ConfirmButton, LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
};

use pdm_api_types::remotes::{NodeUrl, RemoteType};

async fn load_remotes() -> Result<Vec<Remote>, Error> {
    proxmox_yew_comp::http_get("/remotes", None).await
}

async fn delete_item(key: Key) -> Result<(), Error> {
    let id = key.to_string();
    let url = format!("/remotes/{}", percent_encode_component(&id));
    proxmox_yew_comp::http_delete(&url, None).await?;
    Ok(())
}

pub async fn create_remote(mut data: Value, remote_type: RemoteType) -> Result<(), Error> {
    if data.get("nodes").is_none() {
        let nodes = vec![PropertyString::new(NodeUrl {
            hostname: data["hostname"].as_str().unwrap_or_default().to_string(),
            fingerprint: data["fingerprint"].as_str().map(|fp| fp.to_string()),
        })];
        data["nodes"] = serde_json::to_value(nodes)?;
    }
    data["type"] = match remote_type {
        RemoteType::Pve => "pve",
        RemoteType::Pbs => "pbs",
    }
    .into();

    let remote: Remote = serde_json::from_value(data.clone())?;

    let mut params = serde_json::to_value(remote)?;
    if let Some(token) = data["create-token"].as_str() {
        params["create-token"] = token.into();
    }

    proxmox_yew_comp::http_post("/remotes", Some(params)).await
}

/*
async fn update_item(form_ctx: FormContext) -> Result<(), Error> {
    let data = form_ctx.get_submit_data();

    let data = delete_empty_values(&data, &["fingerprint", "comment", "port"], true);

    let name = form_ctx.read().get_field_text("name");

    let url = format!("/config/remote/{}", percent_encode_component(&name));

    proxmox_yew_comp::http_put(&url, Some(data)).await
}
*/

#[derive(PartialEq, Properties)]
pub struct RemoteConfigPanel;

impl RemoteConfigPanel {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

#[derive(PartialEq)]
pub enum ViewState {
    Add(RemoteType),
    Edit,
}

pub enum Msg {
    SelectionChange,
    RemoveItem,
}

pub struct PbsRemoteConfigPanel {
    store: Store<Remote>,
    selection: Selection,
    remote_list_columns: Rc<Vec<DataTableHeader<Remote>>>,
}

impl LoadableComponent for PbsRemoteConfigPanel {
    type Message = Msg;
    type Properties = RemoteConfigPanel;
    type ViewState = ViewState;

    fn load(
        &self,
        _ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        let store = self.store.clone();
        Box::pin(async move {
            let data = load_remotes().await?;
            store.write().set_data(data);
            Ok(())
        })
    }

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        let store = Store::with_extract_key(|record: &Remote| Key::from(record.id.clone()));

        let selection = Selection::new().on_select(ctx.link().callback(|_| Msg::SelectionChange));

        let remote_list_columns = remote_list_columns();

        Self {
            store,
            selection,
            remote_list_columns,
        }
    }

    fn update(&mut self, ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::SelectionChange => true,
            Msg::RemoveItem => {
                if let Some(key) = self.selection.selected_key() {
                    let link = ctx.link();
                    link.clone().spawn(async move {
                        if let Err(err) = delete_item(key).await {
                            link.show_error(tr!("Unable to delete item"), err, true);
                        }
                        link.send_reload();
                    })
                }
                false
            }
        }
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        let link = ctx.link();

        let disabled = self.selection.is_empty();

        let toolbar = Toolbar::new()
            .class("pwt-overflow-hidden")
            .class("pwt-border-bottom")
            .with_child({
                MenuButton::new(tr!("Add")).show_arrow(true).menu(
                    Menu::new()
                        .with_item(
                            MenuItem::new("Proxmox VE")
                                .icon_class("fa fa-building")
                                .on_select(link.change_view_callback(|_| {
                                    Some(ViewState::Add(RemoteType::Pve))
                                })),
                        )
                        .with_item(
                            MenuItem::new("Proxmox Backup Server (Experimental)")
                                .icon_class("fa fa-floppy-o")
                                .on_select(link.change_view_callback(|_| {
                                    Some(ViewState::Add(RemoteType::Pbs))
                                })),
                        ),
                )
            })
            .with_spacer()
            .with_child(
                Button::new(tr!("Edit"))
                    .disabled(disabled)
                    .onclick(link.change_view_callback(|_| Some(ViewState::Edit))),
            )
            .with_child(
                ConfirmButton::new(tr!("Remove"))
                    .confirm_message(tr!("Are you sure you want to remove this remote?"))
                    .disabled(disabled)
                    .on_activate(link.callback(|_| Msg::RemoveItem)),
            )
            .with_flex_spacer()
            .with_child({
                let loading = ctx.loading();
                let link = ctx.link();
                Button::refresh(loading).onclick(move |_| link.send_reload())
            });

        Some(toolbar.into())
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        let columns = Rc::clone(&self.remote_list_columns);
        let link = ctx.link();
        DataTable::new(columns, self.store.clone())
            .class(pwt::css::FlexFit)
            .selection(self.selection.clone())
            .on_row_dblclick(move |_: &mut _| {
                link.change_view(Some(ViewState::Edit));
            })
            .into()
    }

    fn dialog_view(
        &self,
        ctx: &LoadableComponentContext<Self>,
        view_state: &Self::ViewState,
    ) -> Option<Html> {
        match view_state {
            ViewState::Add(ty) => Some(self.create_add_dialog(ctx, *ty)),
            ViewState::Edit => self
                .selection
                .selected_key()
                .map(|key| self.create_edit_dialog(ctx, key)),
        }
    }
}

/*
fn add_remote_input_panel(_form_ctx: &FormContext) -> Html {
    InputPanel::new()
        .padding(4)
        .with_field(tr!("Remote ID"), Field::new().name("id").required(true))
        .with_right_field(
            tr!("Fingerprint"),
            Field::new()
                .name("fingerprint")
                .schema(&CERT_FINGERPRINT_SHA256_SCHEMA),
        )
        .with_field(
            tr!("Server address"),
            Field::new().name("server").required(true),
        )
        .with_field(
            tr!("User/Token"),
            Field::new()
                .name("authid")
                .schema(&pdm_api_types::Authid::API_SCHEMA)
                .required(true),
        )
        .with_field(
            tr!("Password/Secret"),
            Field::new()
                .name("token")
                .input_type(InputType::Password)
                .required(true),
        )
        .into()
}
*/

impl PbsRemoteConfigPanel {
    fn create_add_dialog(
        &self,
        ctx: &LoadableComponentContext<Self>,
        remote_type: RemoteType,
    ) -> Html {
        super::AddWizard::new(remote_type)
            .on_close(ctx.link().change_view_callback(|_| None))
            .on_submit(move |ctx| create_remote(ctx, remote_type))
            .into()

        // EditWindow::new(tr!("Add") + ": " + &tr!("Remote"))
        //     .renderer(add_remote_input_panel)
        //     .on_submit(move |ctx: FormContext| create_item(ctx.get_submit_data(), remote_type))
        //     .on_done(ctx.link().change_view_callback(|_| None))
        //     .into()
    }

    fn create_edit_dialog(&self, ctx: &LoadableComponentContext<Self>, key: Key) -> Html {
        EditRemote::new(&*key)
            .on_done(ctx.link().change_view_callback(|_| None))
            .into()
    }
}

impl Into<VNode> for RemoteConfigPanel {
    fn into(self) -> VNode {
        let comp = VComp::new::<LoadableComponentMaster<PbsRemoteConfigPanel>>(Rc::new(self), None);
        VNode::from(comp)
    }
}

fn remote_list_columns() -> Rc<Vec<DataTableHeader<Remote>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Remote ID"))
            .width("200px")
            .render(|item: &Remote| {
                html! {
                    &item.id
                }
            })
            .sorter(|a: &Remote, b: &Remote| a.id.cmp(&b.id))
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Type"))
            .width("60px")
            .render(|item: &Remote| {
                html! {
                    &item.ty
                }
            })
            .sorter(|a: &Remote, b: &Remote| a.ty.cmp(&b.ty))
            .into(),
        DataTableColumn::new(tr!("AuthId"))
            .width("200px")
            .render(|item: &Remote| {
                html! {
                    &item.authid
                }
            })
            .sorter(|a: &Remote, b: &Remote| a.authid.cmp(&b.authid))
            .into(),
        DataTableColumn::new(tr!("Nodes"))
            .flex(1)
            .render(|item: &Remote| {
                if item.nodes.is_empty() {
                    html! {tr!("None")}
                } else {
                    let nodes = item
                        .nodes
                        .iter()
                        .map(|n| n.hostname.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    let mut tip = Column::new();
                    tip.add_children(item.nodes.iter().map(|n| {
                        let text = match n.fingerprint.clone() {
                            Some(fp) => format!("{} ({fp})", n.hostname),
                            None => n.hostname.to_string(),
                        };
                        html! {<div>{text}</div>}
                    }));
                    Tooltip::new(nodes).rich_tip(tip).into()
                }
            })
            .into(),
        /*
        DataTableColumn::new(tr!("Auth ID"))
            .width("200px")
            .render(|item: &Remote| html!{
                item.config.auth_id.clone()
            })
            .sorter(|a: &Remote, b: &Remote| {
                a.config.auth_id.cmp(&b.config.auth_id)
            })
            .into(),

        DataTableColumn::new(tr!("Fingerprint"))
            .width("200px")
            .render(|item: &Remote| html!{
                item.config.fingerprint.clone().unwrap_or(String::new())
            })
            .into(),

        DataTableColumn::new(tr!("Comment"))
            .flex(1)
            .render(|item: &Remote| html!{
                item.config.comment.clone().unwrap_or(String::new())
            })
            .into()
            */
    ])
}
