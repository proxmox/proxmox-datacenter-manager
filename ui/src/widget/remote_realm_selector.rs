use anyhow::format_err;
use std::rc::Rc;

use yew::html::IntoPropValue;
use yew::prelude::*;

use pwt::props::RenderFn;
use pwt::state::Store;
use pwt::widget::data_table::{DataTable, DataTableColumn, DataTableHeader};
use pwt::widget::form::{Selector, SelectorRenderArgs, ValidateFn};
use pwt::widget::GridPicker;

use proxmox_yew_comp::common_api_types::BasicRealmInfo;
use proxmox_yew_comp::percent_encoding::percent_encode_component;

use pwt::props::{FieldBuilder, WidgetBuilder};
use pwt_macros::{builder, widget};

#[widget(comp=PdmPveRealmSelector, @input)]
#[derive(Clone, Properties, PartialEq)]
#[builder]
pub struct PveRealmSelector {
    /// The default value.
    #[builder(IntoPropValue, into_prop_value)]
    #[prop_or_default]
    pub default: Option<AttrValue>,

    pub hostname: AttrValue,

    pub fingerprint: Option<AttrValue>,
}

impl PveRealmSelector {
    pub fn new(
        hostname: impl IntoPropValue<AttrValue>,
        fingerprint: impl IntoPropValue<Option<AttrValue>>,
    ) -> Self {
        yew::props!(Self {
            hostname: hostname.into_prop_value(),
            fingerprint: fingerprint.into_prop_value(),
        })
    }
}

pub struct PdmPveRealmSelector {
    store: Store<BasicRealmInfo>,
    validate: ValidateFn<(String, Store<BasicRealmInfo>)>,
    picker: RenderFn<SelectorRenderArgs<Store<BasicRealmInfo>>>,
}

impl Component for PdmPveRealmSelector {
    type Message = ();
    type Properties = PveRealmSelector;

    fn create(ctx: &Context<Self>) -> Self {
        let store = Store::new().on_change(ctx.link().callback(|_| ())); // trigger redraw

        let validate = ValidateFn::new(|(realm, store): &(String, Store<BasicRealmInfo>)| {
            if store.read().data().iter().any(|item| &item.realm == realm) {
                Ok(())
            } else {
                Err(format_err!("no such realm"))
            }
        });

        let picker = RenderFn::new({
            let columns = columns();
            move |args: &SelectorRenderArgs<Store<BasicRealmInfo>>| {
                let table = DataTable::new(columns.clone(), args.store.clone()).class("pwt-fit");

                GridPicker::new(table)
                    .selection(args.selection.clone())
                    .on_select(args.controller.on_select_callback())
                    .into()
            }
        });

        Self {
            store,
            validate,
            picker,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();

        let mut url = format!(
            "/pve/realms?hostname={}",
            percent_encode_component(&props.hostname)
        );
        if let Some(fp) = &props.fingerprint {
            url.push_str(&format!("&fingerprint={}", percent_encode_component(fp)));
        }

        Selector::new(self.store.clone(), self.picker.clone())
            .with_std_props(&props.std_props)
            .with_input_props(&props.input_props)
            .required(true)
            .default(props.default.as_deref().unwrap_or("pam").to_string())
            .loader(url)
            .validate(self.validate.clone())
            .into()
    }
}

fn columns() -> Rc<Vec<DataTableHeader<BasicRealmInfo>>> {
    Rc::new(vec![
        DataTableColumn::new("Realm")
            .width("100px")
            .sort_order(true)
            .show_menu(false)
            .get_property(|record: &BasicRealmInfo| &record.realm)
            .into(),
        DataTableColumn::new("Comment")
            .width("300px")
            .show_menu(false)
            .get_property_owned(|record: &BasicRealmInfo| {
                record.comment.clone().unwrap_or_default()
            })
            .into(),
    ])
}
