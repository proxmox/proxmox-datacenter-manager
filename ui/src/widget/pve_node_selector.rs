use std::rc::Rc;

use anyhow::Error;
use proxmox_yew_comp::Status;
use yew::{
    html,
    html::{IntoEventCallback, IntoPropValue},
    virtual_dom::Key,
    AttrValue, Callback, Component, Properties,
};

use pwt::{
    css::FlexFit,
    props::{ContainerBuilder, FieldBuilder, WidgetBuilder, WidgetStyleBuilder},
    state::Store,
    tr,
    widget::{
        data_table::{DataTable, DataTableColumn, DataTableHeader},
        form::{Selector, SelectorRenderArgs},
        GridPicker, Row,
    },
    AsyncPool,
};
use pwt_macros::{builder, widget};

use pdm_client::types::ClusterNodeIndexResponse;

#[widget(comp=PveNodeSelectorComp, @input)]
#[derive(Clone, Properties, PartialEq)]
#[builder]
pub struct PveNodeSelector {
    /// The default value
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub default: Option<AttrValue>,

    /// Change callback
    #[builder_cb(IntoEventCallback, into_event_callback, Option<AttrValue>)]
    #[prop_or_default]
    pub on_change: Option<Callback<Option<AttrValue>>>,

    /// The remote to select the nodes from
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub remote: AttrValue,
}

impl PveNodeSelector {
    pub fn new(remote: impl IntoPropValue<AttrValue>) -> Self {
        yew::props!(Self {
            remote: remote.into_prop_value()
        })
    }
}

pub enum Msg {
    UpdateNodeList(Result<Vec<ClusterNodeIndexResponse>, Error>),
}

pub struct PveNodeSelectorComp {
    _async_pool: AsyncPool,
    store: Store<ClusterNodeIndexResponse>,
    last_err: Option<AttrValue>,
}

impl PveNodeSelectorComp {
    async fn get_node_list(remote: AttrValue) -> Result<Vec<ClusterNodeIndexResponse>, Error> {
        let mut nodes = crate::pdm_client().pve_list_nodes(&remote).await?;
        nodes.sort_by(|a, b| a.node.cmp(&b.node));
        Ok(nodes)
    }
}

impl Component for PveNodeSelectorComp {
    type Message = Msg;
    type Properties = PveNodeSelector;

    fn create(ctx: &yew::Context<Self>) -> Self {
        let _async_pool = AsyncPool::new();
        let remote = ctx.props().remote.clone();
        _async_pool.send_future(ctx.link().clone(), async move {
            Msg::UpdateNodeList(Self::get_node_list(remote).await)
        });
        Self {
            _async_pool,
            last_err: None,
            store: Store::with_extract_key(|node: &ClusterNodeIndexResponse| {
                Key::from(node.node.as_str())
            }),
        }
    }

    fn update(&mut self, _ctx: &yew::Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::UpdateNodeList(res) => match res {
                Ok(result) => self.store.set_data(result),
                Err(err) => self.last_err = Some(err.to_string().into()),
            },
        }

        true
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();
        let err = self.last_err.clone();
        let on_change = {
            let on_change = props.on_change.clone();
            let store = self.store.clone();
            move |key: Key| {
                if let Some(on_change) = &on_change {
                    let result = store
                        .read()
                        .iter()
                        .find(|e| key == store.extract_key(e))
                        .map(|e| e.node.clone().into());
                    on_change.emit(result);
                }
            }
        };
        Selector::new(
            self.store.clone(),
            move |args: &SelectorRenderArgs<Store<ClusterNodeIndexResponse>>| {
                if let Some(err) = &err {
                    return Row::new()
                        .with_child(Status::Error.to_fa_icon())
                        .with_child(err)
                        .into();
                }
                GridPicker::new(
                    DataTable::new(columns(), args.store.clone())
                        .min_width(300)
                        .header_focusable(false)
                        .class(FlexFit),
                )
                .selection(args.selection.clone())
                .on_select(args.controller.on_select_callback())
                .into()
            },
        )
        .with_std_props(&props.std_props)
        .with_input_props(&props.input_props)
        .autoselect(true)
        .editable(true)
        .on_change(on_change)
        .default(props.default.clone())
        .into()
    }
}

fn columns() -> Rc<Vec<DataTableHeader<ClusterNodeIndexResponse>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Node"))
            .get_property(|entry: &ClusterNodeIndexResponse| &entry.node)
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Memory Usage"))
            .render(
                |entry: &ClusterNodeIndexResponse| match (entry.mem, entry.maxmem) {
                    (Some(mem), Some(maxmem)) => {
                        html! {format!("{:.2}%", 100.0 * mem as f64 / maxmem as f64)}
                    }
                    _ => html! {},
                },
            )
            .sorter(|a: &ClusterNodeIndexResponse, b: &ClusterNodeIndexResponse| a.mem.cmp(&b.mem))
            .into(),
    ])
}
