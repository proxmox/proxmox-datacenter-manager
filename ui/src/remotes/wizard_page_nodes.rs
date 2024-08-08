use std::rc::Rc;

use anyhow::Error;
use yew::html::IntoPropValue;
use yew::virtual_dom::{Key, VComp, VNode};

use pwt::prelude::*;
use pwt::state::{Selection, Store};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::{Field, FormContextObserver, InputType};
use pwt::widget::{Button, Column, Container, InputPanel, Row};

use proxmox_yew_comp::{SchemaValidation, WizardPageRenderInfo};

use pdm_api_types::remotes::NodeUrl;
use pdm_api_types::CERT_FINGERPRINT_SHA256_SCHEMA;

use pwt_macros::builder;

use super::ServerInfo;

#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct WizardPageNodes {
    info: WizardPageRenderInfo,

    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    server_info: Option<ServerInfo>,
}

impl WizardPageNodes {
    pub fn new(info: WizardPageRenderInfo) -> Self {
        yew::props!(Self { info })
    }
}

pub enum Msg {
    SelectionChange(Selection),
}
pub struct PdmWizardPageNodes {
    store: Store<NodeUrl>,
    selection: Selection,
    columns: Rc<Vec<DataTableHeader<NodeUrl>>>,
}

impl Component for PdmWizardPageNodes {
    type Message = Msg;
    type Properties = WizardPageNodes;

    fn create(ctx: &Context<Self>) -> Self {
        let props = ctx.props();
        if props.server_info.is_none() {
            props.info.page_lock(true);
        }
        let store = Store::with_extract_key(|item: &NodeUrl| Key::from(item.hostname.clone()));
        if let Some(server_info) = &props.server_info {
            store.write().set_data(server_info.nodes.clone());
        }
        let selection = Selection::new()
            .multiselect(true)
            .on_select(ctx.link().callback(Msg::SelectionChange));
        let columns = Rc::new(columns(selection.clone()));
        Self {
            store,
            selection,
            columns,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::SelectionChange(selection) => {
                ctx.props().info.page_lock(selection.is_empty());
            }
        }
        true
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();
        props.info.page_lock(props.server_info.is_none());
        if let Some(server_info) = &props.server_info {
            self.store.write().set_data(server_info.nodes.clone());
        } else {
            props.info.form_ctx.write().reset_form();
        }
        true
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        let _disabled = self.selection.is_empty();

        DataTable::new(Rc::clone(&self.columns), self.store.clone())
            .class("pwt-flex-fit")
            // Note: We must not hide fields, because they would unregister from the FormContext,
            // so we disable virtual scroll to avoid that.
            .virtual_scroll(false)
            .selection(self.selection.clone())
            .into()
    }
}

impl Into<VNode> for WizardPageNodes {
    fn into(self) -> VNode {
        let comp = VComp::new::<PdmWizardPageNodes>(Rc::new(self), None);
        VNode::from(comp)
    }
}

fn columns(selection: Selection) -> Vec<DataTableHeader<NodeUrl>> {
    vec![
        DataTableColumn::selection_indicator().into(),
        DataTableColumn::new(tr!("Node"))
            .width("200px")
            .render(|item: &NodeUrl| {
                html! {
                    item.hostname.clone()
                }
            })
            .sorter(|a: &NodeUrl, b: &NodeUrl| a.hostname.cmp(&b.hostname))
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Address"))
            .width("400px")
            .render(move |item: &NodeUrl| {
                let selected = selection.contains(&Key::from(item.hostname.clone()));
                Container::new()
                    .with_child(
                        Field::new()
                            .name(format!("node_address_{}", item.hostname))
                            .required(selected),
                    )
                    .onclick(|event: MouseEvent| event.stop_propagation())
                    .ondblclick(|event: MouseEvent| event.stop_propagation())
                    .into()
            })
            .into(),
        DataTableColumn::new(tr!("Fingerprint"))
            .width("200px")
            .render(|item: &NodeUrl| {
                Container::new()
                    .with_child(Field::new().schema(&CERT_FINGERPRINT_SHA256_SCHEMA))
                    .onclick(|event: MouseEvent| event.stop_propagation())
                    .ondblclick(|event: MouseEvent| event.stop_propagation())
                    .into()
            })
            .into(),
    ]
}
