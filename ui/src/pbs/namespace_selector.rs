// TODO: mostly copied from PBS Yew UI prototype but minor adaptions, e.g. better column sizing in
// the picker.

use std::rc::Rc;

use anyhow::format_err;

use yew::html::{IntoEventCallback, IntoPropValue};
use yew::prelude::*;
use yew::virtual_dom::Key;

use pbs_api_types::percent_encoding::percent_encode_component;
use pbs_api_types::NamespaceListItem;

use pwt::props::{FieldBuilder, RenderFn, WidgetBuilder, WidgetStyleBuilder};
use pwt::state::Store;
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::{Selector, SelectorRenderArgs, ValidateFn};
use pwt::widget::GridPicker;

use pwt_macros::{builder, widget};

#[widget(comp=PbsNamespaceSelector, @input)]
#[derive(Clone, Properties, PartialEq)]
#[builder]
pub struct NamespaceSelector {
    url: AttrValue,

    /// The default value.
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub default: Option<AttrValue>,

    /// Change callback
    #[builder_cb(IntoEventCallback, into_event_callback, Key)]
    #[prop_or_default]
    pub on_change: Option<Callback<Key>>,
}

impl NamespaceSelector {
    /// Create a new instance for a PBS remote datastore.
    pub fn new(remote: impl AsRef<str>, datastore: impl AsRef<str>) -> Self {
        let url = format!(
            "/pbs/remotes/{remote}/datastore/{datastore}/namespaces",
            remote = percent_encode_component(remote.as_ref()),
            datastore = percent_encode_component(datastore.as_ref()),
        );
        yew::props!(Self {
            url: AttrValue::from(url)
        })
    }
}

pub struct PbsNamespaceSelector {
    store: Store<NamespaceListItem>,
    validate: ValidateFn<(String, Store<NamespaceListItem>)>,
    picker: RenderFn<SelectorRenderArgs<Store<NamespaceListItem>>>,
}

thread_local! {
    static COLUMNS: Rc<Vec<DataTableHeader<NamespaceListItem>>> = Rc::new(vec![
        DataTableColumn::new("Namespace")
            .flex(4)
            .show_menu(false)
            .render(|item: &NamespaceListItem| {
                let name = item.ns.name();
                if name.is_empty() {
                    html!{"Root"}
                } else {
                    html!{item.ns.name()}
                }
            })
            .into(),
        DataTableColumn::new("Comment")
            .flex(3)
            .show_menu(false)
            .render(|item: &NamespaceListItem| {
                if item.ns.name().is_empty() && item.comment.is_none() {
                    html!{"The Root (default) Namespace."}
                } else {
                    html!{item.comment.clone().unwrap_or(String::new())}
                }
            })
            .into(),
    ]);
}

impl Component for PbsNamespaceSelector {
    type Message = ();
    type Properties = NamespaceSelector;

    fn create(ctx: &Context<Self>) -> Self {
        let store = Store::with_extract_key(|item: &NamespaceListItem| Key::from(item.ns.name()))
            .on_change(ctx.link().callback(|_| ())); // trigger redraw

        let validate = ValidateFn::new(|(ns, store): &(String, Store<NamespaceListItem>)| {
            store
                .read()
                .data()
                .iter()
                .find(|item| &item.ns.name() == ns)
                .map(drop)
                .ok_or_else(|| format_err!("no such namespace"))
        });

        let picker = RenderFn::new(|args: &SelectorRenderArgs<Store<NamespaceListItem>>| {
            let table = DataTable::new(COLUMNS.with(Rc::clone), args.store.clone())
                .class("pwt-fit")
                .min_width(600);

            GridPicker::new(table)
                .selection(args.selection.clone())
                .on_select(args.controller.on_select_callback())
                .into()
        });

        Self {
            store,
            validate,
            picker,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();

        Selector::new(self.store.clone(), self.picker.clone())
            .with_std_props(&props.std_props)
            .with_input_props(&props.input_props)
            .placeholder("Root")
            .default(props.default.clone())
            .loader(&*props.url)
            .validate(self.validate.clone())
            .on_change(props.on_change.clone())
            .into()
    }
}
