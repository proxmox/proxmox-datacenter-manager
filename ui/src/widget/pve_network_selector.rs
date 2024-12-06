use std::rc::Rc;

use anyhow::Error;
use yew::{
    html,
    html::{IntoEventCallback, IntoPropValue},
    virtual_dom::Key,
    AttrValue, Callback, Component, Properties,
};

use pwt::{
    css::FlexFit,
    props::{FieldBuilder, LoadCallback, WidgetBuilder, WidgetStyleBuilder},
    state::Store,
    tr,
    widget::{
        data_table::{DataTable, DataTableColumn, DataTableHeader},
        form::{Selector, SelectorRenderArgs},
        GridPicker,
    },
};
use pwt_macros::{builder, widget};

use pdm_client::types::{ListNetworksType, NetworkInterface};

#[widget(comp=PveNetworkSelectorComp, @input)]
#[derive(Clone, Properties, PartialEq)]
#[builder]
pub struct PveNetworkSelector {
    /// The default value
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub default: Option<AttrValue>,

    /// Change callback
    #[builder_cb(IntoEventCallback, into_event_callback, Option<AttrValue>)]
    #[prop_or_default]
    pub on_change: Option<Callback<Option<AttrValue>>>,

    /// The remote to select the network from
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub remote: AttrValue,

    /// The node to select the network from
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or(AttrValue::from("localhost"))]
    pub node: AttrValue,

    /// The interface types to list
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or(Some(ListNetworksType::AnyBridge))]
    pub interface_type: Option<ListNetworksType>,
}

impl PveNetworkSelector {
    pub fn new(remote: impl IntoPropValue<AttrValue>) -> Self {
        yew::props!(Self {
            remote: remote.into_prop_value()
        })
    }
}

pub struct PveNetworkSelectorComp {
    store: Store<NetworkInterface>,
    load_callback: LoadCallback<Vec<NetworkInterface>>,
}

impl PveNetworkSelectorComp {
    async fn get_network_list(
        remote: AttrValue,
        node: AttrValue,
        ty: Option<ListNetworksType>,
    ) -> Result<Vec<NetworkInterface>, Error> {
        let mut interfaces = crate::pdm_client()
            .pve_list_networks(&remote, &node, ty)
            .await?;
        interfaces.sort_by(|a, b| a.iface.cmp(&b.iface));
        Ok(interfaces)
    }

    fn create_load_callback(ctx: &yew::Context<Self>) -> LoadCallback<Vec<NetworkInterface>> {
        let props = ctx.props();
        let remote = props.remote.clone();
        let node = props.node.clone();
        let ty = props.interface_type;

        (move || Self::get_network_list(remote.clone(), node.clone(), ty)).into()
    }
}

impl Component for PveNetworkSelectorComp {
    type Message = ();
    type Properties = PveNetworkSelector;

    fn create(ctx: &yew::Context<Self>) -> Self {
        Self {
            store: Store::with_extract_key(|iface: &NetworkInterface| {
                Key::from(iface.iface.as_str())
            }),
            load_callback: Self::create_load_callback(ctx),
        }
    }

    fn changed(&mut self, ctx: &yew::Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        if old_props.remote != props.remote
            || old_props.node != props.node
            || old_props.interface_type != props.interface_type
        {
            self.load_callback = Self::create_load_callback(ctx);
        }
        true
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();

        let on_change = {
            let on_change = props.on_change.clone();
            let store = self.store.clone();
            move |key: Key| {
                if let Some(on_change) = &on_change {
                    let result = store
                        .read()
                        .iter()
                        .find(|e| key == store.extract_key(e))
                        .map(|e| e.iface.clone().into());
                    on_change.emit(result);
                }
            }
        };
        Selector::new(
            self.store.clone(),
            move |args: &SelectorRenderArgs<Store<NetworkInterface>>| {
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
        .loader(self.load_callback.clone())
        .with_std_props(&props.std_props)
        .with_input_props(&props.input_props)
        .autoselect(true)
        .on_change(on_change)
        .default(props.default.clone())
        .into()
    }
}

fn columns() -> Rc<Vec<DataTableHeader<NetworkInterface>>> {
    Rc::new(vec![
        DataTableColumn::new(tr!("Bridge"))
            .get_property(|entry: &NetworkInterface| &entry.iface)
            .sort_order(true)
            .into(),
        DataTableColumn::new(tr!("Comment"))
            .render(
                |entry: &NetworkInterface| html! {entry.comments.as_deref().unwrap_or_default()},
            )
            .into(),
    ])
}
