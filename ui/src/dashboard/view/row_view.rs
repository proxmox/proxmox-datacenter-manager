use std::collections::HashMap;
use std::rc::Rc;

use yew::virtual_dom::{VComp, VNode};

use pwt::css;
use pwt::prelude::*;
use pwt::props::RenderFn;
use pwt::widget::{Column, Container, Panel, Row};
use pwt_macros::builder;

use crate::dashboard::types::RowWidget;

#[derive(Properties, PartialEq)]
#[builder]
pub struct RowView {
    rows: Vec<Vec<RowWidget>>,
    widget_renderer: RenderFn<RowWidget>,
}

impl RowView {
    /// Creates a new RowView
    pub fn new(rows: Vec<Vec<RowWidget>>, widget_renderer: impl Into<RenderFn<RowWidget>>) -> Self {
        let widget_renderer = widget_renderer.into();
        yew::props! { Self {rows, widget_renderer }}
    }
}

impl From<RowView> for VNode {
    fn from(val: RowView) -> Self {
        let comp = VComp::new::<RowViewComp>(Rc::new(val), None);
        VNode::from(comp)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
/// Represents the position of a widget in a row view
pub struct Position {
    row: usize,
    item: usize,
}

pub struct RowViewComp {
    current_layout: Vec<Vec<(Position, RowWidget)>>,
}

fn extract_row_layout(rows: &Vec<Vec<RowWidget>>) -> Vec<Vec<(Position, RowWidget)>> {
    rows.iter()
        .enumerate()
        .map(|(row_idx, row)| {
            row.iter()
                .enumerate()
                .map(|(item_idx, item)| {
                    (
                        Position {
                            row: row_idx,
                            item: item_idx,
                        },
                        item.clone(),
                    )
                })
                .collect()
        })
        .collect()
}

impl Component for RowViewComp {
    type Message = ();
    type Properties = RowView;

    fn create(ctx: &Context<Self>) -> Self {
        let current_layout = extract_row_layout(&ctx.props().rows);

        let mut next_row_indices = HashMap::new();
        for (row_idx, row) in current_layout.iter().enumerate() {
            next_row_indices.insert(row_idx, row.len());
        }
        Self { current_layout }
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        if props.rows != old_props.rows {
            self.current_layout = extract_row_layout(&props.rows);
        }

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let mut view = Column::new();
        let layout = &self.current_layout;
        let mut row = Row::new()
            .padding_x(2)
            .class("pwt-content-spacer-colors")
            .class(css::FlexDirection::Row)
            .class(css::FlexWrap::Wrap);
        for (row_idx, items) in layout.iter().enumerate() {
            let flex_sum: f32 = items
                .iter()
                .map(|(_, item)| 1.0f32.max(item.flex.unwrap_or(1.0)))
                .sum();
            let gaps_ratio = 1.0; //items.len().saturating_sub(1) as f32 / items.len() as f32;

            for (_item_idx, (coords, item)) in items.iter().enumerate() {
                let flex = item.flex.unwrap_or(1.0);
                let flex_ratio = 95.0 * (flex.max(1.0)) / flex_sum;
                // we have to subtract the gaps too
                let flex_style = format!(
                    "{} {} calc({}% - calc({} * var(--pwt-spacer-4)))",
                    flex, flex, flex_ratio, gaps_ratio
                );

                let widget = props.widget_renderer.apply(&item);
                let row_element = Panel::new()
                    .border(true)
                    .margin_x(2)
                    .margin_bottom(4)
                    .key(format!("item-{}-{}", coords.row, coords.item))
                    .style("flex", flex_style)
                    .with_child(widget);

                row.add_child(row_element);
            }

            row.add_child(
                Container::new()
                    .key(format!("spacer-{row_idx}"))
                    .style("flex", "1 1 100%"),
            );
        }

        view.add_child(row);
        view.into()
    }
}
