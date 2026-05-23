//! Ceph monitors tab: a read-only table over `GET /ceph/clusters/{id}/mon`.

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

use pdm_client::types::CephMon;

async fn load_mons(cluster: &str) -> Result<Vec<CephMon>, Error> {
    proxmox_yew_comp::http_get(format!("/ceph/clusters/{cluster}/mon"), None).await
}

#[derive(PartialEq, Properties)]
pub struct CephMonitorsPanel {
    cluster: AttrValue,
}

impl CephMonitorsPanel {
    pub fn new(cluster: impl Into<AttrValue>) -> Self {
        yew::props!(Self {
            cluster: cluster.into()
        })
    }
}

pub struct PdmCephMonitorsPanel {
    state: LoadableComponentState<()>,
    store: Store<CephMon>,
    columns: Rc<Vec<DataTableHeader<CephMon>>>,
}

pwt::impl_deref_mut_property!(PdmCephMonitorsPanel, state, LoadableComponentState<()>);

impl LoadableComponent for PdmCephMonitorsPanel {
    type Message = ();
    type Properties = CephMonitorsPanel;
    type ViewState = ();

    fn create(ctx: &LoadableComponentContext<Self>) -> Self {
        ctx.link().repeated_load(15_000);
        let store = Store::with_extract_key(|m: &CephMon| Key::from(m.name.clone()));
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
            let data = load_mons(&cluster).await?;
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

impl From<CephMonitorsPanel> for VNode {
    fn from(val: CephMonitorsPanel) -> Self {
        VNode::from(VComp::new::<LoadableComponentMaster<PdmCephMonitorsPanel>>(
            Rc::new(val),
            None,
        ))
    }
}

fn columns() -> Rc<Vec<DataTableHeader<CephMon>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Name"))
            .flex(1)
            .get_property(|m: &CephMon| &m.name)
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Host"))
            .flex(1)
            .render(|m: &CephMon| html! { { m.host.clone().unwrap_or_default() } })
            .into(),
        DataTableColumn::new(tr!("Quorum"))
            .width("110px")
            .render(|m: &CephMon| {
                let in_quorum = m.quorum.unwrap_or(false);
                let status = if in_quorum {
                    Status::Success
                } else {
                    Status::Error
                };
                let label = if in_quorum { tr!("in") } else { tr!("out") };
                Row::new()
                    .gap(2)
                    .class(AlignItems::Center)
                    .with_child(Fa::from(status))
                    .with_child(html! { { label } })
                    .into()
            })
            .into(),
        DataTableColumn::new(tr!("Address"))
            .flex(2)
            .render(|m: &CephMon| html! { { m.addr.clone().unwrap_or_default() } })
            .into(),
        DataTableColumn::new(tr!("Version"))
            .width("120px")
            .render(|m: &CephMon| html! { { m.ceph_version_short.clone().unwrap_or_default() } })
            .into(),
    ])
}
