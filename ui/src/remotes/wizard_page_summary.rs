use std::rc::Rc;

use yew::virtual_dom::{Key, VComp, VNode};

use pwt::css::FlexFit;
use pwt::prelude::*;
use pwt::state::Store;
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::DisplayField;
use pwt::widget::{Container, InputPanel};

use proxmox_yew_comp::WizardPageRenderInfo;

use proxmox_schema::property_string::PropertyString;

use pdm_api_types::remotes::{NodeUrl, Remote, RemoteType};

use pwt_macros::builder;

#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct WizardPageSummary {
    info: WizardPageRenderInfo,

    #[builder]
    #[prop_or_default]
    server_info: Option<Remote>,

    remote_type: RemoteType,
}

impl WizardPageSummary {
    pub fn new(info: WizardPageRenderInfo, remote_type: RemoteType) -> Self {
        yew::props!(Self { info, remote_type })
    }
}

pub struct PdmWizardPageSummary {
    store: Store<PropertyString<NodeUrl>>,
    columns: Rc<Vec<DataTableHeader<PropertyString<NodeUrl>>>>,
}

impl PdmWizardPageSummary {
    fn set_data(&mut self, ctx: &Context<Self>) {
        self.store.clear();
        let props = ctx.props();

        if let Some(Some(nodes)) = props.info.valid_data.get("nodes").map(|n| n.as_array()) {
            let nodes = nodes
                .into_iter()
                .filter_map(|node| match serde_json::from_value(node.clone()) {
                    Ok(value) => Some(value),
                    Err(err) => {
                        log::error!("could not deserialize: {err}");
                        None
                    }
                })
                .collect();
            self.store.set_data(nodes);
        }
    }
}

impl Component for PdmWizardPageSummary {
    type Message = ();
    type Properties = WizardPageSummary;

    fn create(ctx: &Context<Self>) -> Self {
        let props = ctx.props();
        if props.server_info.is_none() {
            props.info.page_lock(true);
        }

        let store = Store::with_extract_key(|item: &PropertyString<NodeUrl>| {
            Key::from(item.hostname.clone())
        });
        let columns = Rc::new(columns());
        let mut this = Self { store, columns };
        this.set_data(ctx);
        this
    }

    fn changed(&mut self, ctx: &Context<Self>, _old_props: &Self::Properties) -> bool {
        let props = ctx.props();
        props.info.page_lock(props.server_info.is_none());
        self.set_data(ctx);
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let data = &props.info.valid_data;
        let mut input = InputPanel::new()
            .class(FlexFit)
            .padding(4)
            .with_field(
                tr!("Remote ID"),
                DisplayField::new()
                    .value(data["id"].as_str().unwrap_or_default().to_string())
                    .key("remote-id"),
            )
            .with_field(
                tr!("Auth ID"),
                DisplayField::new()
                    .value(data["authid"].as_str().unwrap_or_default().to_string())
                    .key("auth-id"),
            )
            .with_right_field(
                tr!("Create Token"),
                DisplayField::new()
                    .value(match data["create-token"].as_str() {
                        Some(name) => format!("{} ({})", tr!("Yes"), name),
                        None => tr!("No"),
                    })
                    .key("create-token-display"),
            );

        if props.remote_type == RemoteType::Pbs {
            input = input.with_right_field(
                tr!("Hostname"),
                DisplayField::new()
                    .value(data["hostname"].as_str().unwrap_or_default().to_string())
                    .key("hostname"),
            );
        } else {
            input = input
                .with_large_custom_child(
                    Container::new()
                        .key("nodes-title")
                        .padding_top(4)
                        .class("pwt-font-title-medium")
                        .with_child(tr!("Connections")),
                )
                .with_large_custom_child(
                    DataTable::new(Rc::clone(&self.columns), self.store.clone())
                        .key("node-list")
                        .border(true)
                        .class(FlexFit)
                        .max_height(300),
                );
        }

        input.into()
    }
}

impl Into<VNode> for WizardPageSummary {
    fn into(self) -> VNode {
        let comp = VComp::new::<PdmWizardPageSummary>(Rc::new(self), None);
        VNode::from(comp)
    }
}

fn columns() -> Vec<DataTableHeader<PropertyString<NodeUrl>>> {
    vec![
        DataTableColumn::new(tr!("Hostname/Address"))
            .flex(1)
            .render(|item: &PropertyString<NodeUrl>| {
                html! {
                    item.hostname.clone()
                }
            })
            .sorter(|a: &PropertyString<NodeUrl>, b: &PropertyString<NodeUrl>| {
                a.hostname.cmp(&b.hostname)
            })
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Fingerprint"))
            .flex(2)
            .render(move |item: &PropertyString<NodeUrl>| {
                item.fingerprint.as_deref().unwrap_or_default().into()
            })
            .into(),
    ]
}
