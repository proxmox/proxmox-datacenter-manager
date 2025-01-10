//! Streaming snapshot listing.

use std::rc::Rc;

use anyhow::{bail, format_err, Error};
use futures::future::{abortable, AbortHandle};
use yew::virtual_dom::{Key, VComp, VNode};
use yew::Properties;

use pwt::prelude::Context as PwtContext;
use pwt::prelude::{html, tr, Component, Html};
use pwt::state::{Selection, Store};
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};

use pbs_api_types::SnapshotListItem;

use proxmox_yew_comp::http_stream::Stream;

#[derive(Clone, PartialEq, Properties)]
pub struct SnapshotList {
    remote: String,
    datastore: String,
}

impl SnapshotList {
    pub fn new(remote: String, datastore: String) -> Self {
        yew::props!(Self { remote, datastore })
    }
}

impl Into<VNode> for SnapshotList {
    fn into(self) -> VNode {
        let comp = VComp::new::<SnapshotListComp>(Rc::new(self), None);
        VNode::from(comp)
    }
}

enum Msg {
    SelectionChange,
    Data(Vec<SnapshotListItem>),
}

struct SnapshotListComp {
    store: Store<SnapshotListItem>,
    selection: Selection,
    abort: AbortHandle,
    data: Vec<SnapshotListItem>,
}

impl Drop for SnapshotListComp {
    fn drop(&mut self) {
        self.abort.abort();
    }
}

impl SnapshotListComp {
    async fn load_task(
        remote: String,
        datastore: String,
        callback: yew::Callback<Vec<SnapshotListItem>>,
    ) {
        log::info!("starting snapshot listing");
        match list_snapshots(remote, datastore, callback).await {
            Ok(()) => log::info!("done listing snapshots"),
            Err(err) => log::error!("error listing snapshots: {err:?}"),
        }
    }

    fn spawn_load_task(ctx: &PwtContext<Self>) -> AbortHandle {
        let props = ctx.props().clone();
        let callback = ctx.link().callback(Msg::Data);
        let (fut, abort) = abortable(Self::load_task(props.remote, props.datastore, callback));
        wasm_bindgen_futures::spawn_local(async move {
            let _ = fut.await;
        });
        abort
    }
}

impl Component for SnapshotListComp {
    type Message = Msg;
    type Properties = SnapshotList;

    fn create(ctx: &PwtContext<Self>) -> Self {
        let store = Store::with_extract_key(|record: &SnapshotListItem| {
            Key::from(record.backup.to_string())
        });

        let selection = Selection::new().on_select(ctx.link().callback(|_| Msg::SelectionChange));

        let abort = Self::spawn_load_task(ctx);

        Self {
            store,
            selection,
            abort,
            data: Vec::new(),
        }
    }

    fn update(&mut self, _ctx: &PwtContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::SelectionChange => true,
            Msg::Data(data) => {
                self.data.extend(data);
                self.store.set_data(self.data.clone());
                true
            }
        }
    }

    fn view(&self, _ctx: &PwtContext<Self>) -> Html {
        let columns = COLUMNS.with(Rc::clone);
        DataTable::new(columns, self.store.clone())
            .class(pwt::css::FlexFit)
            .selection(self.selection.clone())
            .into()
    }
}

thread_local! {
    static COLUMNS: Rc<Vec<DataTableHeader<SnapshotListItem>>> = {
        Rc::new(
        vec![DataTableColumn::new(tr!("Backup Dir"))
            .flex(1)
            .render(|item: &SnapshotListItem| html! { &item.backup.to_string() })
            .sorter(|a: &SnapshotListItem, b: &SnapshotListItem| a.backup.cmp(&b.backup))
            .sort_order(true)
            .into()]
        )
    };
}

async fn list_snapshots(
    remote: String,
    datastore: String,
    callback: yew::Callback<Vec<SnapshotListItem>>,
) -> Result<(), Error> {
    let client = proxmox_yew_comp::CLIENT.with(|c| std::rc::Rc::clone(&c.borrow()));

    let auth = client
        .get_auth()
        .ok_or_else(|| format_err!("client not authenticated"))?;

    let path = format!("/api2/json/pbs/remotes/{remote}/datastore/{datastore}/snapshots");
    let response = gloo_net::http::Request::get(&path)
        .header("cache-control", "no-cache")
        .header("accept", "application/json-seq")
        .header("CSRFPreventionToken", &auth.csrfprevention_token)
        .send()
        .await?;

    if !response.ok() {
        bail!("snapshot list request failed");
    }

    let raw_reader = response
        .body()
        .ok_or_else(|| format_err!("response contained no body"))?;

    let mut stream = Stream::try_from(raw_reader)?;
    let mut batch = Vec::new();
    while let Some(entry) = stream.next::<pbs_api_types::SnapshotListItem>().await? {
        log::info!("Got a snapshot list entry: {name}", name = entry.backup);
        batch.push(entry);
        if batch.len() > 32 {
            callback.emit(std::mem::take(&mut batch));
        }
    }
    if !batch.is_empty() {
        callback.emit(batch);
    }

    log::info!("finished listing snapshots");

    Ok(())
}
