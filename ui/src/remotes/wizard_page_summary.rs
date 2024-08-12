use std::rc::Rc;

use yew::html::IntoPropValue;
use yew::virtual_dom::{Key, VComp, VNode};

use pwt::prelude::*;
use pwt::state::Store;
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::{Column, Container};

use proxmox_yew_comp::WizardPageRenderInfo;

use pdm_api_types::remotes::NodeUrl;

use pwt_macros::builder;

use super::ServerInfo;

#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct WizardPageSummary {
    info: WizardPageRenderInfo,

    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    server_info: Option<ServerInfo>,
}

impl WizardPageSummary {
    pub fn new(info: WizardPageRenderInfo) -> Self {
        yew::props!(Self { info })
    }
}

pub struct PdmWizardPageNodes {
    store: Store<NodeUrl>,
    columns: Rc<Vec<DataTableHeader<NodeUrl>>>,
}

impl Component for PdmWizardPageNodes {
    type Message = ();
    type Properties = WizardPageSummary;

    fn create(ctx: &Context<Self>) -> Self {
        let props = ctx.props();
        if props.server_info.is_none() {
            props.info.page_lock(true);
        }

        let store = Store::with_extract_key(|item: &NodeUrl| Key::from(item.hostname.clone()));
        if let Some(server_info) = &props.server_info {
            store.write().set_data(server_info.nodes.clone());
        }

        let columns = Rc::new(columns());
        Self { store, columns }
    }

    fn changed(&mut self, ctx: &Context<Self>, _old_props: &Self::Properties) -> bool {
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
        Column::new()
            .with_child(
                Container::new()
                    .padding(4)
                    .border_bottom(true)
                    .with_child(format!("A TEST TEXT")),
            )
            .with_child(
                DataTable::new(Rc::clone(&self.columns), self.store.clone())
                    .class("pwt-flex-fit")
                    .max_height(300),
            )
            .into()
    }
}

impl Into<VNode> for WizardPageSummary {
    fn into(self) -> VNode {
        let comp = VComp::new::<PdmWizardPageNodes>(Rc::new(self), None);
        VNode::from(comp)
    }
}

fn columns() -> Vec<DataTableHeader<NodeUrl>> {
    vec![
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
            .render(move |_item: &NodeUrl| {
                html! {"ADDRESS/Fingerprint"}
            })
            .into(),
    ]
}
