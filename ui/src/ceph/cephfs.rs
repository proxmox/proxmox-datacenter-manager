//! CephFS tab: the file systems from `GET /ceph/clusters/{id}/fs`.
//!
//! The metadata servers backing these file systems live in the Managers tab (alongside the
//! managers), so this tab focuses on the file systems and their data/metadata pools.

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;

use yew::virtual_dom::{Key, VComp, VNode};

use pwt::prelude::*;
use pwt::state::{Selection, Store};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::{Button, Toolbar};

use proxmox_yew_comp::{
    LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState,
};

use pdm_client::types::CephFs;

async fn load_fs(cluster: &str) -> Result<Vec<CephFs>, Error> {
    proxmox_yew_comp::http_get(format!("/ceph/clusters/{cluster}/fs"), None).await
}

#[derive(PartialEq, Properties)]
pub struct CephFsPanel {
    cluster: AttrValue,
}

impl CephFsPanel {
    pub fn new(cluster: impl Into<AttrValue>) -> Self {
        yew::props!(Self {
            cluster: cluster.into()
        })
    }
}

pub struct PdmCephFsPanel {
    state: LoadableComponentState<()>,
    store: Store<CephFs>,
    // row-highlight only, no action on select
    selection: Selection,
    columns: Rc<Vec<DataTableHeader<CephFs>>>,
}

pwt::impl_deref_mut_property!(PdmCephFsPanel, state, LoadableComponentState<()>);

impl LoadableComponent for PdmCephFsPanel {
    type Message = ();
    type Properties = CephFsPanel;
    type ViewState = ();

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        ctx.link().repeated_load(15_000);
        let store = Store::with_extract_key(|f: &CephFs| Key::from(f.name.clone()));
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
            let data = load_fs(&cluster).await?;
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

impl From<CephFsPanel> for VNode {
    fn from(val: CephFsPanel) -> Self {
        VNode::from(VComp::new::<LoadableComponentMaster<PdmCephFsPanel>>(
            Rc::new(val),
            None,
        ))
    }
}

/// The data pools of a file system: prefer the full `data_pools` list, falling back to the single
/// `data_pool` field.
fn data_pools(fs: &CephFs) -> String {
    match &fs.data_pools {
        Some(pools) if !pools.is_empty() => pools.join(", "),
        _ => fs.data_pool.clone(),
    }
}

fn columns() -> Rc<Vec<DataTableHeader<CephFs>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Name"))
            .flex(1)
            .get_property(|f: &CephFs| &f.name)
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Data Pools"))
            .flex(2)
            .render(|f: &CephFs| html! { { data_pools(f) } })
            .into(),
        DataTableColumn::new(tr!("Metadata Pool"))
            .flex(1)
            .get_property(|f: &CephFs| &f.metadata_pool)
            .into(),
    ])
}
