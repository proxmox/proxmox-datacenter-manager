use std::rc::Rc;

use anyhow::Error;
use yew::html::IntoPropValue;

use pwt::widget::form::Combobox;
use pwt::{prelude::*, AsyncPool};

use pwt_macros::{builder, widget};

use crate::{pdm_client, RemoteList};

static PREDEFINED_PATHS: &[&str] = &[
    "/",
    "/access",
    "/access/acl",
    "/access/users",
    "/resource",
    "/system",
    "/system/certificates",
    "/system/disks",
    "/system/log",
    "/system/network",
    "/system/network/dns",
    "/system/network/interfaces",
    "/system/notifications",
    "/system/services",
    "/system/status",
    "/system/tasks",
    "/system/time",
    "/view",
];

#[widget(comp=PdmPermissionPathSelector, @input, @element)]
#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct PermissionPathSelector {
    /// Default value
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    default: Option<AttrValue>,
}

impl PermissionPathSelector {
    pub(super) fn new() -> Self {
        yew::props!(Self {})
    }
}

enum Msg {
    Prefetched(Vec<String>),
    PrefetchFailed,
}

struct PdmPermissionPathSelector {
    items: Rc<Vec<AttrValue>>,
    _async_pool: AsyncPool,
}

impl PdmPermissionPathSelector {
    async fn get_view_paths() -> Result<Vec<String>, Error> {
        let views = pdm_client().list_views().await?;
        let paths: Vec<String> = views
            .iter()
            .map(|cfg| format!("/view/{}", cfg.id))
            .collect();
        Ok(paths)
    }

    async fn get_paths(remote_list: Option<RemoteList>) -> Result<Vec<String>, Error> {
        let mut paths = Self::get_view_paths().await?;

        if let Some(remotes) = remote_list {
            let mut remote_paths = remotes
                .0
                .iter()
                .map(|remote| format!("/resource/{}", remote.id))
                .collect();
            paths.append(&mut remote_paths);
        }

        Ok(paths)
    }
}

impl Component for PdmPermissionPathSelector {
    type Message = Msg;
    type Properties = PermissionPathSelector;

    fn create(ctx: &Context<Self>) -> Self {
        let base_items: Vec<AttrValue> = PREDEFINED_PATHS
            .iter()
            .map(|i| AttrValue::from(*i))
            .collect();

        let result = ctx.link().context(Callback::from(|_: RemoteList| {}));
        let remote_list = result.map(|(remote_list, _)| remote_list);

        let async_pool = AsyncPool::new();
        let link = ctx.link().clone();

        async_pool.spawn(async move {
            let paths = Self::get_paths(remote_list).await;
            match paths {
                Ok(paths) => link.send_message(Msg::Prefetched(paths)),
                Err(_) => link.send_message(Msg::PrefetchFailed),
            }
        });

        Self {
            items: Rc::new(base_items),
            _async_pool: async_pool,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Prefetched(paths) => {
                let items = Rc::make_mut(&mut self.items);
                items.extend(paths.into_iter().map(AttrValue::from));
                items.sort_by_key(|k| k.to_lowercase());
                true
            }
            Msg::PrefetchFailed => false,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        Combobox::new()
            .with_std_props(&props.std_props)
            .with_input_props(&props.input_props)
            .default(props.default.clone())
            .items(Rc::clone(&self.items))
            .editable(true)
            .into()
    }
}
