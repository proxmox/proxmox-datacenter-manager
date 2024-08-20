//! Streaming snapshot listing.

use std::rc::Rc;

use anyhow::{bail, format_err, Error};
use yew::virtual_dom::{VComp, VNode};
use yew::Properties;

use pwt::convert_js_error;
use pwt::prelude::{html, Component, Context, Html};

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

enum Msg {}

struct SnapshotListComp {}

impl Component for SnapshotListComp {
    type Message = Msg;
    type Properties = SnapshotList;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {}
    }

    fn update(&mut self, _ctx: &Context<Self>, _msg: Self::Message) -> bool {
        false
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! { format!("Showing pbs {remote}", remote = ctx.props().remote) }
    }
}

async fn list_snapshots(remote: String, datastore: String) -> Result<(), Error> {
    let client = proxmox_yew_comp::CLIENT.with(|c| std::rc::Rc::clone(&c.borrow()));

    let auth = client
        .get_auth()
        .ok_or_else(|| format_err!("client not authenticated"))?;
    proxmox_yew_comp::set_cookie(&auth.ticket.cookie());

    let mut path = format!("/api2/json/pbs/{remote}/datastore/{datastore}/snapshots");
    let response = gloo_net::http::Request::get(&path)
        .header("cache-control", "no-cache")
        .header("accept", "application/json-seq")
        .header("CSRFPreventionToken", &auth.csrfprevention_token)
        .send()
        .await?;

    if !response.ok() {
        bail!("snapshot list request failed");
    }

    let reader = response
        .body()
        .ok_or_else(|| format_err!("response contained no body"))?
        .get_reader();

    Ok(())
}
