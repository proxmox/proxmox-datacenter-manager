/*
mod wizard_page_connect;
use add_wizard::AddWizard;
use wizard_page_connect::WizardPageConnect;

mod wizard_page_nodes;
use wizard_page_nodes::WizardPageNodes;

mod wizard_page_summary;
pub use wizard_page_summary::WizardPageSummary;

mod add_wizard;
*/

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::{bail, Error};
use pwt::widget::form::{Field, FormContext, InputType};
use serde::{Deserialize, Serialize};

//use proxmox_yew_comp::percent_encoding::percent_encode_component;
use pdm_api_types::remotes::Remote;
use proxmox_schema::{property_string::PropertyString, ApiType};
use proxmox_yew_comp::SchemaValidation;

use pbs_api_types::{
    CERT_FINGERPRINT_SHA256_SCHEMA, DNS_NAME_OR_IP_SCHEMA, REMOTE_ID_SCHEMA, REMOTE_PASSWORD_SCHEMA,
};

//use proxmox_schema::api_types::{CERT_FINGERPRINT_SHA256_SCHEMA, DNS_NAME_OR_IP_SCHEMA};

use serde_json::Value;
use yew::virtual_dom::{Key, VComp, VNode};

use pwt::prelude::*;
use pwt::state::{Selection, Store};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
//use pwt::widget::form::{delete_empty_values, Field, FormContext, InputType};
use pwt::widget::{Button, InputPanel, Toolbar};

use proxmox_yew_comp::{
    EditWindow, LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
};

use pdm_api_types::remotes::{NodeUrl, RemoteType};

/// Data returned by connect call.
#[derive(Clone, PartialEq)]
pub struct ServerInfo {
    remote_type: RemoteType,
    nodes: Vec<NodeUrl>,
}

#[derive(Deserialize, Serialize)]
/// Parameters for connect call.
pub struct ConnectParams {
    server: String,
    username: String,
    password: String,
    fingerprint: Option<String>,
}

async fn load_remotes() -> Result<Vec<Remote>, Error> {
    proxmox_yew_comp::http_get("/remotes", None).await
}
/*
async fn delete_item(key: Key) -> Result<(), Error> {
    let id = key.to_string();
    let url = format!("/config/remote/{}", percent_encode_component(&id));
    proxmox_yew_comp::http_delete(&url, None).await?;
    Ok(())
}
*/

async fn create_item(form_ctx: FormContext) -> Result<(), Error> {
    let mut data = form_ctx.get_submit_data();

    data["type"] = "pve".into();
    data["nodes"] = Value::Array(Vec::new());

    let fingerprint = match data.as_object_mut().unwrap().remove("fingerprint") {
        Some(Value::String(fingerprint)) => Some(fingerprint),
        _ => None,
    };

    let hostname = match data.as_object_mut().unwrap().remove("server") {
        Some(Value::String(server)) => server,
        _ => bail!("missing server address"),
    };

    let mut remote: Remote = serde_json::from_value(data)?;

    let node = NodeUrl {
        hostname,
        fingerprint,
    };
    remote.nodes = vec![PropertyString::new(node)];

    proxmox_yew_comp::http_post("/remotes", Some(serde_json::to_value(remote).unwrap())).await
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
    Add,
    //Edit,
}

pub enum Msg {
    SelectionChange,
}

pub struct PbsRemoteConfigPanel {
    store: Store<Remote>,
    selection: Selection,
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

        Self { store, selection }
    }

    fn update(&mut self, _ctx: &LoadableComponentContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::SelectionChange => true,
        }
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        let link = ctx.link();

        let _disabled = self.selection.is_empty();

        let toolbar = Toolbar::new()
            .class("pwt-overflow-hidden")
            .class("pwt-border-bottom")
            .with_child({
                Button::new(tr!("Add")).onclick(link.change_view_callback(|_| Some(ViewState::Add)))
            })
            /*
            .with_spacer()
            .with_child(
                Button::new(tr!("Edit"))
                    .disabled(disabled)
                    .onclick(link.change_view_callback(|_| Some(ViewState::Edit))),
            )
            .with_child(
                Button::new(tr!("Remove"))
                    .disabled(disabled)
                    .onclick(link.callback(|_| Msg::RemoveItem)),
            )
            */
            .with_flex_spacer()
            .with_child({
                let loading = ctx.loading();
                let link = ctx.link();
                Button::refresh(loading).onclick(move |_| link.send_reload())
            });

        Some(toolbar.into())
    }

    fn main_view(&self, _ctx: &LoadableComponentContext<Self>) -> Html {
        let columns = COLUMNS.with(Rc::clone);
        DataTable::new(columns, self.store.clone())
            .class("pwt-flex-fit")
            .selection(self.selection.clone())
            //.on_row_dblclick(move |_: &mut _| {
            //    link.change_view(Some(ViewState::Edit));
            //})
            .into()
    }

    fn dialog_view(
        &self,
        ctx: &LoadableComponentContext<Self>,
        view_state: &Self::ViewState,
    ) -> Option<Html> {
        match view_state {
            ViewState::Add => Some(self.create_add_dialog(ctx)),
        }
    }
}

fn add_remote_input_panel(form_ctx: &FormContext) -> Html {
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

impl PbsRemoteConfigPanel {
    fn create_add_dialog(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        /*
        AddWizard::new()
            .on_close(ctx.link().change_view_callback(|_| None))
            .into()
        */
        EditWindow::new(tr!("Add") + ": " + &tr!("Remote"))
            .renderer(add_remote_input_panel)
            .on_submit(create_item)
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

thread_local! {
    static COLUMNS: Rc<Vec<DataTableHeader<Remote>>> = Rc::new(vec![
        DataTableColumn::new(tr!("Remote ID"))
            .width("200px")
            .render(|item: &Remote| html!{
                &item.id
            })
            .sorter(|a: &Remote, b: &Remote| {
                a.id.cmp(&b.id)
            })
            .sort_order(true)
            .into(),

            DataTableColumn::new(tr!("Type"))
            .width("50px")
            .render(|item: &Remote| html!{
                &item.ty
            })
            .sorter(|a: &Remote, b: &Remote| {
                a.ty.cmp(&b.ty)
            })
            .into(),
        DataTableColumn::new(tr!("AuthId"))
            .width("200px")
            .render(|item: &Remote| html!{
                &item.authid
            })
            .sorter(|a: &Remote, b: &Remote| {
                a.authid.cmp(&b.authid)
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
    ]);
}
