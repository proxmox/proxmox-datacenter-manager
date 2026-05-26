use std::rc::Rc;

use anyhow::{bail, Error};
use proxmox_yew_comp::{rrd_value_renderer, Status};
use yew::{
    html,
    html::{IntoEventCallback, IntoPropValue},
    virtual_dom::Key,
    AttrValue, Callback, Component, Properties,
};

use pwt::{
    css::{FlexFit, Opacity},
    props::{ContainerBuilder, FieldBuilder, WidgetBuilder, WidgetStyleBuilder},
    state::Store,
    tr,
    widget::{
        data_table::{DataTable, DataTableColumn, DataTableHeader, DataTableRowRenderArgs},
        form::{Selector, SelectorRenderArgs},
        Fa, GridPicker, Row,
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

    /// Node that should be rendered as the current source (greyed out, suffixed with
    /// "(current)") and rejected by validation. Used by the migration dialog so the user
    /// cannot pick the guest's current node as a target.
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

    fn apply_filter(&mut self, excluded: &[String]) {
        let filtered: Vec<ClusterNodeIndexResponse> = if excluded.is_empty() {
            self.raw_nodes.clone()
        } else {
            self.raw_nodes
                .iter()
                .filter(|n| !excluded.iter().any(|e| e == &n.node))
                .cloned()
                .collect()
        };
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
                    self.apply_filter(&ctx.props().excluded_nodes);
                }
                Err(err) => self.last_err = Some(err.to_string().into()),
            },
        }

        true
    }

    fn changed(&mut self, ctx: &yew::Context<Self>, old_props: &Self::Properties) -> bool {
        if old_props.excluded_nodes != ctx.props().excluded_nodes {
            self.apply_filter(&ctx.props().excluded_nodes);
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
        let mut selector = Selector::new(self.store.clone(), {
            let source_node = source_node.clone();
            move |args: &SelectorRenderArgs<Store<ClusterNodeIndexResponse>>| {
                if let Some(err) = &err {
                    return Row::new()
                        .with_child(Fa::from(Status::Error))
                        .with_child(err)
                        .into();
                }
                let source_node_for_row = source_node.clone();
                GridPicker::new(
                    DataTable::new(
                        columns(show_memory, source_node.clone()),
                        args.store.clone(),
                    )
                    .min_width(300)
                    .header_focusable(false)
                    // dim the source node so the user sees why it is in the list but
                    // cannot pick it; selection itself is blocked via `validate` below
                    .row_render_callback(
                        move |args: &mut DataTableRowRenderArgs<ClusterNodeIndexResponse>| {
                            if let Some(src) = &source_node_for_row {
                                if args.record().node == src.as_str() {
                                    args.add_class(Opacity::Half);
                                }
                            }
                        },
                    )
                    .class(FlexFit),
                )
                .selection(args.selection.clone())
                .on_select(args.controller.on_select_callback())
                .into()
            }
        })
        .with_std_props(&props.std_props)
        .with_input_props(&props.input_props)
        // Skip autoselect when migrating intra-cluster so the dialog does not open already
        // pointed at the (rejected) source row; cross-remote callers (source_node: None) keep
        // the convenience of a seeded selection.
        .autoselect(source_node.is_none())
        .editable(true)
        .on_change(on_change)
        .default(props.default.clone());

        if let Some(src) = source_node {
            // reject the source node so an accidental click can't submit a no-op migration
            selector = selector.validate(
                move |(value, _store): &(String, Store<ClusterNodeIndexResponse>)| {
                    if value == src.as_str() {
                        bail!(tr!("Cannot migrate to the guest's current node"));
                    }
                    Ok(())
                },
            );
        }

        selector.into()
    }
}

fn columns(
    show_memory: bool,
    source_node: Option<AttrValue>,
) -> Rc<Vec<DataTableHeader<ClusterNodeIndexResponse>>> {
    let node_column = if let Some(source_node) = source_node {
        DataTableColumn::new(tr!("Node"))
            .render(move |entry: &ClusterNodeIndexResponse| {
                if entry.node == source_node.as_str() {
                    html! { tr!("{0} (current)", entry.node) }
                } else {
                    html! { entry.node.clone() }
                }
            })
            .sorter(
                |a: &ClusterNodeIndexResponse, b: &ClusterNodeIndexResponse| a.node.cmp(&b.node),
            )
            .sort_order(true)
            .into()
    } else {
        DataTableColumn::new(tr!("Node"))
            .get_property(|entry: &ClusterNodeIndexResponse| &entry.node)
            .sort_order(true)
            .into()
    };
    let mut columns = vec![node_column];
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
