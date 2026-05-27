//! A [`GridPicker`]-based selector for access tokens for the automated installer.

use serde_json::Value;
use std::{collections::HashSet, rc::Rc};
use yew::{html, virtual_dom::Key, Properties};

use pdm_api_types::auto_installer::AnswerToken;
use pwt::{
    css::FlexFit,
    prelude::*,
    state::{Selection, Store},
    widget::{
        data_table::{DataTable, DataTableColumn, DataTableHeader, MultiSelectMode},
        form::{
            ManagedField, ManagedFieldContext, ManagedFieldMaster, ManagedFieldScopeExt,
            ManagedFieldState,
        },
        GridPicker,
    },
};
use pwt_macros::{builder, widget};

#[widget(comp = ManagedFieldMaster<TokenSelectorField>, @input)]
#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct TokenSelector {
    /// All available tokens to select.
    store: Store<AnswerToken>,

    #[builder]
    #[prop_or_default]
    /// Keys of entries to pre-select.
    pub selected_keys: Vec<String>,
}

impl TokenSelector {
    pub fn new(store: Store<AnswerToken>) -> Self {
        yew::props!(Self { store })
    }
}

pub struct TokenSelectorField {
    state: ManagedFieldState,
    store: Store<AnswerToken>,
    selection: Selection,
    columns: Rc<Vec<DataTableHeader<AnswerToken>>>,
}

pwt::impl_deref_mut_property!(TokenSelectorField, state, ManagedFieldState);

pub enum Message {
    UpdateSelection,
}

impl ManagedField for TokenSelectorField {
    type Message = Message;
    type Properties = TokenSelector;
    type ValidateClosure = ();

    fn create(ctx: &ManagedFieldContext<Self>) -> Self {
        let selection = Selection::new()
            .multiselect(true)
            .on_select(ctx.link().callback(|_| Message::UpdateSelection));

        let mut selected = ctx.props().selected_keys.clone();
        selected.sort();

        let store = ctx.props().store.clone().on_change(ctx.link().callback({
            // re-apply selection when store changes
            let selection = selection.clone();
            let selected = selected
                .iter()
                .cloned()
                .map(Key::from)
                .collect::<HashSet<Key>>();
            move |_| {
                selection.bulk_select(selected.clone());
                Message::UpdateSelection
            }
        }));

        let default = serde_json::to_value(selected).unwrap_or_default();

        Self {
            state: ManagedFieldState::new(default.clone(), default),
            store,
            selection,
            columns: Self::columns(),
        }
    }

    fn validation_args(_props: &Self::Properties) -> Self::ValidateClosure {}

    fn validator(_props: &Self::ValidateClosure, value: &Value) -> Result<Value, anyhow::Error> {
        Ok(value.clone())
    }

    fn update(&mut self, ctx: &ManagedFieldContext<Self>, msg: Self::Message) -> bool {
        match msg {
            Self::Message::UpdateSelection => {
                let mut selected = self
                    .selection
                    .selected_keys()
                    .iter()
                    .map(|k| k.to_string())
                    .collect::<Vec<_>>();
                selected.sort();
                ctx.link().update_value(selected);
                true
            }
        }
    }

    fn value_changed(&mut self, _ctx: &ManagedFieldContext<Self>) {
        // The Selection is the picker's source of truth; re-sync it whenever the field value
        // changes externally - notably a form reset, which rewrites the value but would otherwise
        // leave the checkboxes where the user left them.
        let keys: HashSet<Key> = serde_json::from_value::<Vec<String>>(self.state.value.clone())
            .unwrap_or_default()
            .into_iter()
            .map(Key::from)
            .collect();
        self.selection.bulk_select(keys);
    }

    fn changed(&mut self, ctx: &ManagedFieldContext<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        if old_props.selected_keys != props.selected_keys {
            let mut selected = props
                .selected_keys
                .iter()
                .map(|k| k.to_string())
                .collect::<Vec<_>>();
            selected.sort();

            ctx.link().update_default(selected);
        }

        true
    }

    fn view(&self, _ctx: &ManagedFieldContext<Self>) -> Html {
        GridPicker::new(
            DataTable::new(self.columns.clone(), self.store.clone())
                .multiselect_mode(MultiSelectMode::Simple)
                .border(true)
                .class(FlexFit),
        )
        .selection(self.selection.clone())
        .into()
    }
}

impl TokenSelectorField {
    fn columns() -> Rc<Vec<DataTableHeader<AnswerToken>>> {
        Rc::new(vec![
            DataTableColumn::selection_indicator().into(),
            DataTableColumn::new(tr!("Token"))
                .flex(1)
                .render(|item: &AnswerToken| html! { &item.id })
                .sorter(|a: &AnswerToken, b: &AnswerToken| a.id.cmp(&b.id))
                .sort_order(true)
                .into(),
            DataTableColumn::new(tr!("Comment"))
                .flex(1)
                .render(|item: &AnswerToken| html! { item.comment.as_deref().unwrap_or("") })
                .into(),
        ])
    }
}
