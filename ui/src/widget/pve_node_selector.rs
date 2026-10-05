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
        container::div,
        data_table::{DataTable, DataTableColumn, DataTableHeader},
        form::{Selector, SelectorRenderArgs},
    },
};
use pwt_macros::{builder, widget};

use pdm_api_types::resource::{PveNodeResource, ResourceView};
use pdm_client::types::ClusterResourceKind;

use crate::renderer::render_resource_extra_info;

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
    UpdateNodeList(Result<Vec<PveNodeResource>, Error>),
}

pub struct PveNodeSelectorComp {
    _async_pool: AsyncPool,
    store: Store<PveNodeResource>,
    /// Unfiltered node list as fetched from the remote, kept so a prop change to `excluded_nodes`
    /// can re-filter without round-tripping the remote again.
    raw_nodes: Vec<PveNodeResource>,
    last_err: Option<AttrValue>,
}

impl PveNodeSelectorComp {
    async fn get_node_list(remote: AttrValue) -> Result<Vec<PveNodeResource>, Error> {
        let mut nodes: Vec<PveNodeResource> = crate::pdm_client()
            .pve_cluster_resources(&remote, Some(ClusterResourceKind::Node))
            .await?
            .into_iter()
            .filter_map(|r| match r {
                pdm_api_types::resource::PveResource::Node(node) => Some(node),
                _ => None,
            })
            .collect();
        nodes.sort_by(|a, b| a.node.cmp(&b.node));
        Ok(nodes)
    }

    fn apply_filter(&mut self, excluded: &[String], source_node: Option<&str>) {
        let filtered: Vec<PveNodeResource> = self
            .raw_nodes
            .iter()
            .filter(|n| {
                !excluded.iter().any(|e| e == &n.node) && source_node != Some(n.node.as_str())
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
            store: Store::with_extract_key(|node: &PveNodeResource| Key::from(node.node.as_str())),
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
            move |args: &SelectorRenderArgs<Store<PveNodeResource>>| {
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

fn columns(show_memory: bool) -> Rc<Vec<DataTableHeader<PveNodeResource>>> {
    let mut columns = vec![
        DataTableColumn::new(tr!("Node"))
            .width("250px")
            .render(|entry: &PveNodeResource| {
                Row::new()
                    .gap(1)
                    .with_child(div(&entry.node))
                    .with_optional_child(render_resource_extra_info(ResourceView::PveNode(entry)))
                    .into()
            })
            .sorter(|a: &PveNodeResource, b: &PveNodeResource| a.node.cmp(&b.node))
            .sort_order(true)
            .into(),
    ];
    if show_memory {
        columns.push(
            DataTableColumn::new(tr!("CPU Usage"))
                .render(|entry: &PveNodeResource| {
                    if entry.status != "online" {
                        return html! {};
                    }
                    rrd_value_renderer::render_cpu_usage(&entry.cpu).into()
                })
                .sorter(|a: &PveNodeResource, b: &PveNodeResource| {
                    let a = if a.status == "online" {
                        a.cpu
                    } else {
                        f64::NEG_INFINITY
                    };
                    let b = if b.status == "online" {
                        b.cpu
                    } else {
                        f64::NEG_INFINITY
                    };
                    a.total_cmp(&b)
                })
                .into(),
        );
        columns.push(
            DataTableColumn::new(tr!("Memory Usage"))
                .render(|entry: &PveNodeResource| {
                    // don't show values when the node isn't online or when it's non sensible
                    if entry.maxmem == 0 || entry.status != "online" {
                        return html! {};
                    }
                    html! {format!("{:.2}%", 100.0 * entry.mem as f64 / entry.maxmem as f64)}
                })
                .sorter(|a: &PveNodeResource, b: &PveNodeResource| {
                    let a = (a.mem as f64) / (a.maxmem as f64);
                    let b = (b.mem as f64) / (b.maxmem as f64);
                    a.total_cmp(&b)
                })
                .into(),
        );
    }
    Rc::new(columns)
}
