use std::rc::Rc;

use anyhow::Error;
use proxmox_yew_comp::{Status, rrd_value_renderer};
use yew::{
    AttrValue, Callback, Component, Properties, html,
    html::{IntoEventCallback, IntoPropValue},
    virtual_dom::Key,
};

use pwt::{
    AsyncPool,
    css::FlexFit,
    props::{ContainerBuilder, FieldBuilder, WidgetBuilder, WidgetStyleBuilder},
    state::Store,
    tr,
    widget::{
        Fa, GridPicker, Row,
        data_table::{DataTable, DataTableColumn, DataTableHeader},
        form::{Selector, SelectorRenderArgs},
    },
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

    /// Node names that should not appear in the selector (e.g. nodes that already have a
    /// subscription key assigned in the pool).
    #[prop_or_default]
    pub excluded_nodes: Rc<Vec<String>>,

    /// Whether to show the resource-utilization columns ("CPU Usage", "Memory Usage"). Callers
    /// picking a node for a context where utilization is irrelevant (e.g. subscription
    /// assignment) can hide them.
    #[builder]
    #[prop_or(true)]
    pub show_memory: bool,

    /// Source node of the guest. Used by the migration dialog to hide the node from the
    /// target list, so the user can only pick a different node as the migration target.
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub source_node: Option<AttrValue>,
}

impl PveNodeSelector {
    pub fn new(remote: impl IntoPropValue<AttrValue>) -> Self {
        yew::props!(Self {
            remote: remote.into_prop_value()
        })
    }

    pub fn excluded_nodes(mut self, nodes: Rc<Vec<String>>) -> Self {
        self.excluded_nodes = nodes;
        self
    }
}

pub enum Msg {
    UpdateNodeList(Result<Vec<ClusterNodeIndexResponse>, Error>),
}

pub struct PveNodeSelectorComp {
    _async_pool: AsyncPool,
    store: Store<ClusterNodeIndexResponse>,
    /// Unfiltered node list as fetched from the remote, kept so a prop change to `excluded_nodes`
    /// can re-filter without round-tripping the remote again.
    raw_nodes: Vec<ClusterNodeIndexResponse>,
    last_err: Option<AttrValue>,
}

impl PveNodeSelectorComp {
    async fn get_node_list(remote: AttrValue) -> Result<Vec<ClusterNodeIndexResponse>, Error> {
        let mut nodes = crate::pdm_client().pve_list_nodes(&remote).await?;
        nodes.sort_by(|a, b| a.node.cmp(&b.node));
        Ok(nodes)
    }

    fn apply_filter(&mut self, excluded: &[String], source_node: Option<&str>) {
        let filtered: Vec<ClusterNodeIndexResponse> = self
            .raw_nodes
            .iter()
            .filter(|n| {
                !excluded.iter().any(|e| e == &n.node)
                    && source_node != Some(n.node.as_str())
            })
            .cloned()
            .collect();
        self.store.set_data(filtered);
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
            raw_nodes: Vec::new(),
            store: Store::with_extract_key(|node: &ClusterNodeIndexResponse| {
                Key::from(node.node.as_str())
            }),
        }
    }

    fn update(&mut self, ctx: &yew::Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::UpdateNodeList(res) => match res {
                Ok(result) => {
                    self.raw_nodes = result;
                    self.apply_filter(
                        &ctx.props().excluded_nodes,
                        ctx.props().source_node.as_deref(),
                    );
                }
                Err(err) => self.last_err = Some(err.to_string().into()),
            },
        }

        true
    }

    fn changed(&mut self, ctx: &yew::Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();
        if old_props.excluded_nodes != props.excluded_nodes
            || old_props.source_node != props.source_node
        {
            self.apply_filter(&props.excluded_nodes, props.source_node.as_deref());
        }
        true
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();
        let err = self.last_err.clone();
        let show_memory = props.show_memory;
        let source_node = props.source_node.clone();
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
        Selector::new(self.store.clone(), {
            move |args: &SelectorRenderArgs<Store<ClusterNodeIndexResponse>>| {
                if let Some(err) = &err {
                    return Row::new()
                        .with_child(Fa::from(Status::Error))
                        .with_child(err)
                        .into();
                }
                GridPicker::new(
                    DataTable::new(columns(show_memory), args.store.clone())
                        .min_width(300)
                        .header_focusable(false)
                        .class(FlexFit),
                )
                .selection(args.selection.clone())
                .on_select(args.controller.on_select_callback())
                .into()
            }
        })
        .with_std_props(&props.std_props)
        .with_input_props(&props.input_props)
        .autoselect(source_node.is_none())
        .editable(true)
        .on_change(on_change)
        .default(props.default.clone())
        .into()
    }
}

fn columns(show_memory: bool) -> Rc<Vec<DataTableHeader<ClusterNodeIndexResponse>>> {
    let mut columns = vec![
        DataTableColumn::new(tr!("Node"))
            .get_property(|entry: &ClusterNodeIndexResponse| &entry.node)
            .sort_order(true)
            .into(),
    ];
    if show_memory {
        columns.push(
            DataTableColumn::new(tr!("CPU Usage"))
                .render(|entry: &ClusterNodeIndexResponse| match entry.cpu {
                    Some(cpu) => html! { rrd_value_renderer::render_cpu_usage(&cpu) },
                    None => html! {},
                })
                .sorter(
                    |a: &ClusterNodeIndexResponse, b: &ClusterNodeIndexResponse| {
                        // total_cmp tolerates NaN; preserve the "no data sorts low" intuition by
                        // mapping None to negative infinity so unprobed nodes stay at the bottom.
                        a.cpu
                            .unwrap_or(f64::NEG_INFINITY)
                            .total_cmp(&b.cpu.unwrap_or(f64::NEG_INFINITY))
                    },
                )
                .into(),
        );
        columns.push(
            DataTableColumn::new(tr!("Memory Usage"))
                .render(
                    |entry: &ClusterNodeIndexResponse| match (entry.mem, entry.maxmem) {
                        (Some(mem), Some(maxmem)) => {
                            html! {format!("{:.2}%", 100.0 * mem as f64 / maxmem as f64)}
                        }
                        _ => html! {},
                    },
                )
                .sorter(
                    |a: &ClusterNodeIndexResponse, b: &ClusterNodeIndexResponse| a.mem.cmp(&b.mem),
                )
                .into(),
        );
    }
    Rc::new(columns)
}
