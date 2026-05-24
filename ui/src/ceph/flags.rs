//! Ceph OSD flags tab: a read-only table over `GET /ceph/clusters/{id}/flags`.

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use anyhow::Error;

use yew::virtual_dom::{Key, VComp, VNode};

use pwt::css::AlignItems;
use pwt::prelude::*;
use pwt::state::Store;
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::{Button, Fa, Row, Toolbar};

use proxmox_yew_comp::{
    LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState, Status,
};

use pdm_client::types::CephFlagInfo;

async fn load_flags(cluster: &str) -> Result<Vec<CephFlagInfo>, Error> {
    proxmox_yew_comp::http_get(format!("/ceph/clusters/{cluster}/flags"), None).await
}

#[derive(PartialEq, Properties)]
pub struct CephFlagsPanel {
    cluster: AttrValue,
}

impl CephFlagsPanel {
    pub fn new(cluster: impl Into<AttrValue>) -> Self {
        yew::props!(Self {
            cluster: cluster.into()
        })
    }
}

pub struct PdmCephFlagsPanel {
    state: LoadableComponentState<()>,
    store: Store<CephFlagInfo>,
    columns: Rc<Vec<DataTableHeader<CephFlagInfo>>>,
}

pwt::impl_deref_mut_property!(PdmCephFlagsPanel, state, LoadableComponentState<()>);

impl LoadableComponent for PdmCephFlagsPanel {
    type Message = ();
    type Properties = CephFlagsPanel;
    type ViewState = ();

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        ctx.link().repeated_load(15_000);
        let store = Store::with_extract_key(|f: &CephFlagInfo| Key::from(f.name.to_string()));
        Self {
            state: LoadableComponentState::new(),
            store,
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
            let data = load_flags(&cluster).await?;
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
            .into()
    }
}

impl From<CephFlagsPanel> for VNode {
    fn from(val: CephFlagsPanel) -> Self {
        VNode::from(VComp::new::<LoadableComponentMaster<PdmCephFlagsPanel>>(
            Rc::new(val),
            None,
        ))
    }
}

fn columns() -> Rc<Vec<DataTableHeader<CephFlagInfo>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Flag"))
            .width("160px")
            .render(|f: &CephFlagInfo| html! { { f.name.to_string() } })
            .into(),
        DataTableColumn::new(tr!("State"))
            .width("110px")
            .render(|f: &CephFlagInfo| {
                // A set flag is operationally noteworthy (e.g. noout during maintenance), so draw
                // attention to it rather than mark green.
                let status = if f.value {
                    Status::Warning
                } else {
                    Status::Success
                };
                let label = if f.value { tr!("set") } else { tr!("unset") };
                Row::new()
                    .gap(2)
                    .class(AlignItems::Center)
                    .with_child(Fa::from(status))
                    .with_child(html! { { label } })
                    .into()
            })
            .into(),
        DataTableColumn::new(tr!("Description"))
            .flex(1)
            .render(|f: &CephFlagInfo| html! { { f.description.clone() } })
            .into(),
    ])
}
