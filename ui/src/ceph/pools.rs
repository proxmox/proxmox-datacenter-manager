//! Ceph pools tab: a read-only table over `GET /ceph/clusters/{id}/pools`.

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;

use yew::virtual_dom::{Key, VComp, VNode};

use proxmox_human_byte::HumanByte;

use pwt::prelude::*;
use pwt::state::{Selection, Store};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::{Button, Toolbar};

use proxmox_yew_comp::{
    LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState,
};

use pdm_client::types::CephPool;

use super::renderer::usage_cell;

async fn load_pools(cluster: &str) -> Result<Vec<CephPool>, Error> {
    proxmox_yew_comp::http_get(format!("/ceph/clusters/{cluster}/pools"), None).await
}

/// The enabled applications for a pool. Ceph reports these as an object keyed by application name
/// (`{"rbd": {...}}`), so the keys are the names.
fn pool_applications(p: &CephPool) -> String {
    p.application_metadata
        .as_ref()
        .and_then(|v| v.as_object())
        .map(|m| m.keys().cloned().collect::<Vec<_>>().join(", "))
        .unwrap_or_default()
}

#[derive(PartialEq, Properties)]
pub struct CephPoolsPanel {
    cluster: AttrValue,
}

impl CephPoolsPanel {
    pub fn new(cluster: impl Into<AttrValue>) -> Self {
        yew::props!(Self {
            cluster: cluster.into()
        })
    }
}

pub struct PdmCephPoolsPanel {
    state: LoadableComponentState<()>,
    store: Store<CephPool>,
    // row-highlight only, no action on select
    selection: Selection,
    columns: Rc<Vec<DataTableHeader<CephPool>>>,
}

pwt::impl_deref_mut_property!(PdmCephPoolsPanel, state, LoadableComponentState<()>);

impl LoadableComponent for PdmCephPoolsPanel {
    type Message = ();
    type Properties = CephPoolsPanel;
    type ViewState = ();

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        ctx.link().repeated_load(15_000);
        let store = Store::with_extract_key(|p: &CephPool| Key::from(p.pool_name.clone()));
        Self {
            state: LoadableComponentState::new(),
            store,
            selection: Selection::new(),
            columns: columns(),
        }
    }

    fn load(
        &self,
        ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>>>> {
        let cluster = ctx.props().cluster.clone();
        let store = self.store.clone();
        Box::pin(async move {
            let data = load_pools(&cluster).await?;
            store.write().set_data(data);
            Ok(())
        })
    }

    fn toolbar(&self, ctx: &LoadableComponentContext<Self>) -> Option<Html> {
        let link = ctx.link().clone();
        let loading = self.loading();
        Some(
            Toolbar::new()
                .class("pwt-overflow-hidden")
                .class("pwt-border-bottom")
                .with_flex_spacer()
                .with_child(Button::refresh(loading).onclick(move |_| link.send_reload()))
                .into(),
        )
    }

    fn main_view(&self, _ctx: &LoadableComponentContext<Self>) -> Html {
        DataTable::new(Rc::clone(&self.columns), self.store.clone())
            .class(pwt::css::FlexFit)
            .selection(self.selection.clone())
            .into()
    }
}

impl From<CephPoolsPanel> for VNode {
    fn from(val: CephPoolsPanel) -> Self {
        VNode::from(VComp::new::<LoadableComponentMaster<PdmCephPoolsPanel>>(
            Rc::new(val),
            None,
        ))
    }
}

fn columns() -> Rc<Vec<DataTableHeader<CephPool>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Name"))
            .flex(2)
            .get_property(|p: &CephPool| &p.pool_name)
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Type"))
            .width("100px")
            .render(|p: &CephPool| {
                let label = match p.ty.to_string().as_str() {
                    "replicated" => tr!("replicated"),
                    "erasure" => tr!("erasure"),
                    other => other.to_string(),
                };
                html! { { label } }
            })
            .into(),
        DataTableColumn::new(tr!("Size/Min"))
            .width("100px")
            .render(|p: &CephPool| html! { { format!("{}/{}", p.size, p.min_size) } })
            .into(),
        DataTableColumn::new(tr!("PG num"))
            .width("90px")
            .justify("right")
            .get_property_owned(|p: &CephPool| p.pg_num)
            .into(),
        DataTableColumn::new(tr!("Used"))
            .flex(1)
            .sorter(|a: &CephPool, b: &CephPool| {
                a.percent_used
                    .unwrap_or_default()
                    .total_cmp(&b.percent_used.unwrap_or_default())
            })
            .render(|p: &CephPool| {
                let pct = p.percent_used.unwrap_or(0.0) * 100.0;
                let used = HumanByte::from(p.bytes_used.unwrap_or(0).max(0) as u64);
                usage_cell(tr!("{0}% ({1})", format!("{:.2}", pct), used), pct)
            })
            .into(),
        DataTableColumn::new(tr!("Autoscale"))
            .width("110px")
            .render(|p: &CephPool| {
                let label = match p.pg_autoscale_mode.as_deref() {
                    Some("on") => tr!("on"),
                    Some("warn") => tr!("warn"),
                    Some("off") => tr!("off"),
                    Some(other) => other.to_string(),
                    None => String::new(),
                };
                html! { { label } }
            })
            .into(),
        DataTableColumn::new(tr!("CRUSH rule"))
            .flex(1)
            .render(|p: &CephPool| {
                html! {
                    { p.crush_rule_name.clone().unwrap_or_else(|| p.crush_rule.to_string()) }
                }
            })
            .into(),
        DataTableColumn::new(tr!("Application"))
            .flex(1)
            .render(|p: &CephPool| html! { { pool_applications(p) } })
            .into(),
    ])
}
