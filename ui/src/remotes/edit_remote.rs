use std::rc::Rc;

use anyhow::Error;
use serde_json::Value;

use yew::html::{IntoEventCallback, Scope};
use yew::virtual_dom::{Key, VComp, VNode};

use pwt::state::Store;
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::{Field, FormContext, InputType};
use pwt::widget::{Button, Column, InputPanel, Toolbar};
use pwt::{css, prelude::*};

use proxmox_yew_comp::percent_encoding::percent_encode_component;
use proxmox_yew_comp::{EditWindow, SchemaValidation};

use pdm_api_types::remotes::{NodeUrl, Remote};
use proxmox_schema::property_string::PropertyString;
use proxmox_schema::ApiType;

use pbs_api_types::CERT_FINGERPRINT_SHA256_SCHEMA;

use pwt_macros::builder;

async fn load_remote(url: AttrValue, link: Scope<PdmEditRemote>) -> Result<Value, Error> {
    let remote: Remote = proxmox_yew_comp::http_get(&*url, None).await?;

    let nodes: Vec<NodeUrl> = remote
        .nodes
        .iter()
        .map(|item| item.clone().into_inner())
        .collect();
    link.send_message(Msg::LoadNodes(nodes));

    Ok(serde_json::to_value(remote)?)
}

#[derive(PartialEq, Properties)]
#[builder]
pub struct EditRemote {
    remote_id: String,
    /// Done callback, called after Close, Abort or Submit.
    #[builder_cb(IntoEventCallback, into_event_callback, ())]
    #[prop_or_default]
    pub on_done: Option<Callback<()>>,
}

impl EditRemote {
    pub fn new(remote_id: &str) -> Self {
        yew::props!(Self {
            remote_id: remote_id.to_owned()
        })
    }
}

pub enum Msg {
    Reset,
    LoadNodes(Vec<NodeUrl>),
    UpdateHostname(usize, String),
    UpdateFingerprint(usize, String),
}

#[derive(PartialEq, Clone, Debug)]
struct IndexedNodeUrl {
    index: usize,
    data: NodeUrl,
}

pub struct PdmEditRemote {
    node_url_list_columns: Rc<Vec<DataTableHeader<IndexedNodeUrl>>>,
    nodes: Store<IndexedNodeUrl>,
    loaded_nodes: Vec<NodeUrl>, // required to reset the form
}

impl PdmEditRemote {
    fn set_nodes(&mut self, nodes: Vec<NodeUrl>) {
        self.nodes.set_data(
            nodes
                .into_iter()
                .enumerate()
                .map(|(index, data)| IndexedNodeUrl { index, data })
                .collect(),
        );
    }
}

impl Component for PdmEditRemote {
    type Message = Msg;
    type Properties = EditRemote;

    fn create(ctx: &Context<Self>) -> Self {
        let node_url_list_columns = node_url_list_columns(ctx.link().clone());
        let nodes = Store::with_extract_key(|item: &IndexedNodeUrl| Key::from(item.index));
        Self {
            node_url_list_columns,
            nodes,
            loaded_nodes: Vec::new(),
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Reset => {
                self.set_nodes(self.loaded_nodes.clone());
                true
            }
            Msg::LoadNodes(nodes) => {
                self.loaded_nodes = nodes;
                self.set_nodes(self.loaded_nodes.clone());
                true
            }
            Msg::UpdateHostname(index, hostname) => {
                let mut data = self.nodes.write();
                if let Some(item) = data.get_mut(index) {
                    item.data.hostname = hostname;
                }
                //log::info!("DATA {:?}", data.data());
                true
            }
            Msg::UpdateFingerprint(index, fingerprint) => {
                let mut data = self.nodes.write();
                if let Some(item) = data.get_mut(index) {
                    if fingerprint.is_empty() {
                        item.data.fingerprint = None;
                    } else {
                        item.data.fingerprint = Some(fingerprint);
                    }
                }
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let node_url_list_columns = Rc::clone(&self.node_url_list_columns);
        let nodes = self.nodes.clone();
        EditWindow::new(tr!("Edit") + ": " + &tr!("Remote"))
            .min_height(400)
            .on_done(props.on_done.clone())
            .on_reset(ctx.link().callback(|_| Msg::Reset))
            .loader((
                {
                    let link = ctx.link().clone();
                    move |url: AttrValue| load_remote(url, link.clone())
                },
                format!(
                    "/remotes/{}/config",
                    percent_encode_component(&props.remote_id)
                ),
            ))
            .renderer(move |form_ctx: &FormContext| {
                edit_remote_input_panel(form_ctx, Rc::clone(&node_url_list_columns), nodes.clone())
            })
            .on_submit({
                let nodes = self.nodes.clone();
                let url = format!("/remotes/{}", percent_encode_component(&props.remote_id));
                move |form_ctx: FormContext| {
                    let nodes = nodes.clone();
                    let url = url.clone();
                    async move {
                        let mut data = form_ctx.get_submit_data();
                        let nodes: Vec<PropertyString<NodeUrl>> = nodes
                            .read()
                            .iter()
                            .map(|item| PropertyString::new(item.data.clone()))
                            .collect();
                        data["nodes"] = serde_json::to_value(nodes)?;
                        proxmox_yew_comp::http_put(&url, Some(data)).await?;
                        Ok(())
                    }
                }
            })
            .into()
    }
}

fn edit_remote_input_panel(
    _form_ctx: &FormContext,
    columns: Rc<Vec<DataTableHeader<IndexedNodeUrl>>>,
    nodes: Store<IndexedNodeUrl>,
) -> Html {
    let toolbar =
        Toolbar::new()
            .margin_x(2)
            .with_flex_spacer()
            .with_child(Button::new(tr!("Add")).onclick({
                let nodes = nodes.clone();
                move |_| {
                    let mut nodes = nodes.write();
                    let index = nodes.len();

                    nodes.push(IndexedNodeUrl {
                        index,
                        data: NodeUrl {
                            hostname: String::new(),
                            fingerprint: None,
                        },
                    })
                }
            }));

    Column::new()
        .class(css::FlexFit)
        .with_child(
            InputPanel::new()
                .class("pwt-w-100")
                .padding(4)
                .with_field(tr!("Remote ID"), Field::new().name("id").required(true))
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
                ),
        )
        .with_child(DataTable::new(columns, nodes).margin(4).border(true))
        .with_child(toolbar)
        .into()
}

impl Into<VNode> for EditRemote {
    fn into(self) -> VNode {
        let comp = VComp::new::<PdmEditRemote>(Rc::new(self), None);
        VNode::from(comp)
    }
}

fn node_url_list_columns(link: Scope<PdmEditRemote>) -> Rc<Vec<DataTableHeader<IndexedNodeUrl>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Hostname"))
            .width("200px")
            .render({
                let link = link.clone();
                move |item: &IndexedNodeUrl| {
                    let index = item.index;
                    Field::new()
                        .name(format!("__field_{index}_hostname__"))
                        .submit(false)
                        .on_change(link.callback(move |value| Msg::UpdateHostname(index, value)))
                        .required(true)
                        .default(item.data.hostname.clone())
                        .into()
                }
            })
            .sorter(|a: &IndexedNodeUrl, b: &IndexedNodeUrl| a.data.hostname.cmp(&b.data.hostname))
            .sort_order(None)
            .into(),
        DataTableColumn::new(tr!("Fingerprint"))
            .width("400px")
            .render({
                let link = link.clone();
                move |item: &IndexedNodeUrl| {
                    let index = item.index;
                    let fingerprint = match &item.data.fingerprint {
                        Some(fingerprint) => fingerprint,
                        None => "",
                    };
                    Field::new()
                        .name(format!("__field_{index}_fingerprint__"))
                        .submit(false)
                        .schema(&CERT_FINGERPRINT_SHA256_SCHEMA)
                        .on_change(link.callback(move |value| Msg::UpdateFingerprint(index, value)))
                        .default(fingerprint.to_string())
                        .into()
                }
            })
            .into(),
    ])
}
