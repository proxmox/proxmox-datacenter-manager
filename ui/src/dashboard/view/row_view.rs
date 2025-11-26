use std::collections::HashMap;
use std::rc::Rc;

use gloo_timers::callback::Timeout;
use wasm_bindgen::JsCast;
use web_sys::Element;
use yew::html::{IntoEventCallback, IntoPropValue};
use yew::virtual_dom::{VComp, VNode};

use pwt::css;
use pwt::prelude::*;
use pwt::props::RenderFn;
use pwt::state::{SharedState, SharedStateObserver};
use pwt::widget::menu::{Menu, MenuButton, MenuItem};
use pwt::widget::{ActionIcon, Button, Column, Container, Row, Tooltip};
use pwt_macros::builder;

use crate::dashboard::types::{RowWidget, ViewLayout, WidgetType};
use crate::dashboard::view::row_element::RowElement;
use crate::dashboard::view::EditingMessage;

use pdm_api_types::remotes::RemoteType;

#[derive(Properties, PartialEq)]
#[builder]
pub struct RowView {
    rows: Vec<Vec<RowWidget>>,
    widget_renderer: RenderFn<RowWidget>,

    #[prop_or_default]
    #[builder(IntoPropValue, into_prop_value)]
    /// If set, enables/disables editing mode
    editing_state: Option<SharedState<Vec<EditingMessage>>>,

    #[prop_or_default]
    #[builder_cb(IntoEventCallback, into_event_callback, ViewLayout)]
    /// Will be called if there is an [`EditingController`] and the editing
    /// is finished.
    on_update_layout: Option<Callback<ViewLayout>>,
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

pub enum OverEvent {
    Pointer(PointerEvent),
    Drag(DragEvent),
}

pub enum DragMsg {
    Start(Position),
    End,
    DragOver(OverEvent, Position),
    Enter(Position),
}

pub enum MoveDirection {
    Up,
    Down,
}
pub enum Msg {
    DragEvent(DragMsg),
    AddRow,
    RemoveRow(usize), // idx
    EditFlex(Position, u32),
    AddWidget(Position, WidgetType),
    RemoveWidget(Position),
    MoveRow(usize, MoveDirection), // idx
    HandleEditMessages,
}

#[derive(Clone, Copy, Debug, PartialEq)]
/// Represents the position of a widget in a row view
pub struct Position {
    row: usize,
    item: usize,
}

pub struct RowViewComp {
    current_layout: Vec<Vec<(Position, RowWidget)>>,
    new_layout: Option<Vec<Vec<(Position, RowWidget)>>>,
    dragging: Option<Position>,        // index of item
    dragging_target: Option<Position>, // index of item
    drag_timeout: Option<Timeout>,

    next_row_indices: HashMap<usize, usize>, // for saving the max index for new widgets

    node_ref: NodeRef,
    edit_mode: bool,
    _editing_observer: Option<SharedStateObserver<Vec<EditingMessage>>>,
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
    type Message = Msg;
    type Properties = RowView;

    fn create(ctx: &Context<Self>) -> Self {
        let current_layout = extract_row_layout(&ctx.props().rows);

        let mut next_row_indices = HashMap::new();
        for (row_idx, row) in current_layout.iter().enumerate() {
            next_row_indices.insert(row_idx, row.len());
        }

        let _editing_observer = ctx
            .props()
            .editing_state
            .as_ref()
            .map(|state| state.add_listener(ctx.link().callback(|_| Msg::HandleEditMessages)));

        Self {
            new_layout: None,
            current_layout,
            dragging: None,
            dragging_target: None,
            drag_timeout: None,
            next_row_indices,
            node_ref: NodeRef::default(),
            edit_mode: false,
            _editing_observer,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::RemoveRow(idx) => {
                self.current_layout.remove(idx);
            }
            Msg::AddRow => {
                self.current_layout.push(Vec::new());
            }
            Msg::DragEvent(drag_msg) => match drag_msg {
                DragMsg::Start(coords) => {
                    self.dragging = Some(coords);
                    self.dragging_target = Some(coords);
                }
                DragMsg::End => {
                    self.dragging = None;
                    self.dragging_target = None;
                    if let Some(layout) = self.new_layout.take() {
                        self.current_layout = layout;
                    }
                }
                DragMsg::DragOver(event, position) => {
                    // check if the pointer is at a position where the item can be dropped
                    // without flickering, namely where it fits from it's dimensions
                    let (target, pointer_pos) = match event {
                        OverEvent::Pointer(event) => (
                            event.target().and_then(|t| t.dyn_into::<Element>().ok()),
                            (event.client_x(), event.client_y()),
                        ),
                        OverEvent::Drag(event) => (
                            event.target().and_then(|t| t.dyn_into::<Element>().ok()),
                            (event.client_x(), event.client_y()),
                        ),
                    };
                    if let Some(el) = self.node_ref.cast::<Element>() {
                        if let Ok(Some(dragging_el)) = el.query_selector(".dragging-item") {
                            let dragging_rect = dragging_el.get_bounding_client_rect();

                            if let Some(target) = target {
                                let target_rect = target.get_bounding_client_rect();

                                let x = pointer_pos.0 as f64;
                                let x_min = target_rect.x();
                                let x_max = target_rect.x() + dragging_rect.width();

                                let y = pointer_pos.1 as f64;
                                let y_min = target_rect.y();
                                let y_max = target_rect.y() + dragging_rect.height();

                                if x >= x_min && x <= x_max && y >= y_min && y <= y_max {
                                    ctx.link()
                                        .send_message(Msg::DragEvent(DragMsg::Enter(position)));
                                }
                            }
                        }
                    }
                }
                DragMsg::Enter(coords) => {
                    if let Some(source_coords) = self.dragging {
                        let mut new_layout = self.current_layout.clone();
                        let item = new_layout[source_coords.row].remove(source_coords.item);
                        let target_idx = new_layout[coords.row].len().min(coords.item);
                        new_layout[coords.row].insert(target_idx, item);
                        self.new_layout = Some(new_layout);
                    }
                    self.dragging_target = Some(coords);
                }
            },
            Msg::EditFlex(coords, flex) => {
                self.current_layout[coords.row][coords.item].1.flex = Some(flex as f32);
            }
            Msg::AddWidget(coords, widget_type) => {
                let next_idx = *self.next_row_indices.get(&coords.row).unwrap_or(&0);
                self.next_row_indices
                    .insert(coords.row, next_idx.saturating_add(1));
                self.current_layout[coords.row].insert(
                    coords.item,
                    (
                        Position {
                            row: coords.row,
                            item: next_idx,
                        },
                        RowWidget {
                            flex: None,
                            title: None,
                            r#type: widget_type,
                        },
                    ),
                );
            }
            Msg::RemoveWidget(coords) => {
                self.current_layout[coords.row].remove(coords.item);
            }
            Msg::MoveRow(old, direction) => {
                let mut new_layout = self.current_layout.clone();
                let row = new_layout.remove(old);
                let new_idx = match direction {
                    MoveDirection::Up => old.saturating_sub(1),
                    MoveDirection::Down => old.saturating_add(1).min(new_layout.len()),
                };
                new_layout.insert(new_idx, row);
                self.current_layout = new_layout;
            }
            Msg::HandleEditMessages => {
                let props = ctx.props();
                let state = match props.editing_state.clone() {
                    Some(state) => state,
                    None => return false,
                };

                if state.read().len() == 0 {
                    return false;
                } // Note: avoid endless loop

                let list = state.write().split_off(0);
                let mut editing = self.edit_mode;
                let mut trigger_finish = false;
                let mut cancel = false;
                for msg in list {
                    match msg {
                        EditingMessage::Start => editing = true,
                        EditingMessage::Cancel => {
                            if editing {
                                cancel = true;
                            }
                            editing = false;
                        }
                        EditingMessage::Finish => {
                            if editing {
                                trigger_finish = true;
                            }
                            editing = false;
                        }
                    }
                }
                if let (true, Some(on_update_layout)) = (trigger_finish, &props.on_update_layout) {
                    let rows = self
                        .current_layout
                        .iter()
                        .map(|row| row.iter().map(|(_, item)| item.clone()).collect())
                        .collect();
                    on_update_layout.emit(ViewLayout::Rows { rows });
                }
                if cancel {
                    self.current_layout = extract_row_layout(&props.rows);
                }
                self.edit_mode = editing;
                if !self.edit_mode {
                    self.dragging = None;
                    self.dragging_target = None;
                    self.drag_timeout = None;
                }
            }
        }
        true
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        let props = ctx.props();
        if props.rows != old_props.rows {
            let new_layout = extract_row_layout(&props.rows);
            if new_layout != self.current_layout {
                self.current_layout = new_layout;
            }
        }

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let mut view = Column::new().onpointerup(
            (self.dragging.is_some() && self.edit_mode)
                .then_some(ctx.link().callback(|_| Msg::DragEvent(DragMsg::End))),
        );
        let layout = self.new_layout.as_ref().unwrap_or(&self.current_layout);
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

            for (item_idx, (coords, item)) in items.iter().enumerate() {
                let flex = item.flex.unwrap_or(1.0);
                let flex_ratio = 95.0 * (flex.max(1.0)) / flex_sum;
                // we have to subtract the gaps too
                let flex_style = format!(
                    "{} {} calc({}% - calc({} * var(--pwt-spacer-4)))",
                    flex, flex, flex_ratio, gaps_ratio
                );
                let current_coords = Position {
                    row: row_idx,
                    item: item_idx,
                };

                let row_element = RowElement::new(item.clone(), props.widget_renderer.clone())
                    .margin_x(2)
                    .margin_bottom(4)
                    .edit_mode(self.edit_mode)
                    .is_dragging(self.dragging_target == Some(current_coords))
                    .key(format!("item-{}-{}", coords.row, coords.item))
                    .style("flex", flex_style)
                    .style("touch-action", self.edit_mode.then_some("none"))
                    .on_remove(
                        ctx.link()
                            .callback(move |_| Msg::RemoveWidget(current_coords)),
                    )
                    .on_flex_change(
                        ctx.link()
                            .callback(move |flex| Msg::EditFlex(current_coords, flex)),
                    )
                    .ondragstart(ctx.link().callback(move |event: DragEvent| {
                        let data = event.data_transfer().unwrap();
                        let _ = data.clear_data();
                        let _ = data.set_data("", "");
                        Msg::DragEvent(DragMsg::Start(current_coords))
                    }))
                    .onpointerdown(self.edit_mode.then_some(ctx.link().callback(
                        move |event: PointerEvent| {
                            // we need to release the pointer capture to trigger pointer events
                            // on other elements
                            if let Some(target) = event
                                .target()
                                .and_then(|target| target.dyn_into::<Element>().ok())
                            {
                                let _ = target.release_pointer_capture(event.pointer_id());
                            }
                            Msg::DragEvent(DragMsg::Start(current_coords))
                        },
                    )))
                    .ondragend(ctx.link().callback(|_| Msg::DragEvent(DragMsg::End)))
                    .onpointermove((self.dragging.is_some() && self.edit_mode).then_some(
                        ctx.link().callback(move |event: PointerEvent| {
                            Msg::DragEvent(DragMsg::DragOver(
                                OverEvent::Pointer(event),
                                current_coords,
                            ))
                        }),
                    ))
                    .ondragover((self.dragging.is_some() && self.edit_mode).then_some(
                        ctx.link().callback(move |event: DragEvent| {
                            Msg::DragEvent(DragMsg::DragOver(
                                OverEvent::Drag(event),
                                current_coords,
                            ))
                        }),
                    ))
                    .ondragover(|event: DragEvent| event.prevent_default())
                    .ondrop(ctx.link().callback(|event: DragEvent| {
                        event.prevent_default();
                        Msg::DragEvent(DragMsg::End)
                    }));

                row.add_child(row_element);
            }

            if self.edit_mode {
                let drop_coords = Position {
                    row: row_idx,
                    item: items.len().saturating_sub(1),
                };
                let is_first_row = row_idx == 0;
                let is_last_row = row_idx == (layout.len().saturating_sub(1));
                row.add_child(
                    Container::new()
                        .key(format!("row-add-{}", row_idx))
                        .style("flex", "1 1 100%")
                        .margin_x(2)
                        .margin_bottom(4)
                        .padding_bottom(4)
                        .border_bottom(true)
                        .ondragenter(
                            ctx.link()
                                .callback(move |_| Msg::DragEvent(DragMsg::Enter(drop_coords))),
                        )
                        .onpointerenter(
                            (self.dragging.is_some() && self.edit_mode)
                                .then_some(ctx.link().callback(move |_| {
                                    Msg::DragEvent(DragMsg::Enter(drop_coords))
                                })),
                        )
                        // necessary for drop event to trigger
                        .ondragover(|event: DragEvent| event.prevent_default())
                        .ondrop(ctx.link().callback(|event: DragEvent| {
                            event.prevent_default();
                            Msg::DragEvent(DragMsg::End)
                        }))
                        .with_child(
                            Row::new()
                                .gap(2)
                                .with_child(
                                    MenuButton::new(tr!("Add Widget"))
                                        .class(css::ColorScheme::Primary)
                                        .show_arrow(true)
                                        .icon_class("fa fa-plus-circle")
                                        .menu(create_menu(
                                            ctx,
                                            Position {
                                                row: row_idx,
                                                item: items.len(),
                                            },
                                        )),
                                )
                                .with_child(
                                    Button::new(tr!("Remove Row"))
                                        .icon_class("fa fa-times")
                                        .class(css::ColorScheme::Error)
                                        .on_activate(
                                            ctx.link().callback(move |_| Msg::RemoveRow(row_idx)),
                                        ),
                                )
                                .with_flex_spacer()
                                .with_child(
                                    Tooltip::new(
                                        ActionIcon::new("fa fa-arrow-down")
                                            .on_activate(ctx.link().callback(move |_| {
                                                Msg::MoveRow(row_idx, MoveDirection::Down)
                                            }))
                                            .disabled(is_last_row),
                                    )
                                    .tip(tr!("Move Row down")),
                                )
                                .with_child(
                                    Tooltip::new(
                                        ActionIcon::new("fa fa-arrow-up")
                                            .on_activate(ctx.link().callback(move |_| {
                                                Msg::MoveRow(row_idx, MoveDirection::Up)
                                            }))
                                            .disabled(is_first_row),
                                    )
                                    .tip(tr!("Move Row up")),
                                ),
                        ),
                );
            }
            row.add_child(
                Container::new()
                    .key(format!("spacer-{row_idx}"))
                    .style("flex", "1 1 100%"),
            );
        }
        if self.edit_mode {
            row.add_child(
                Container::new()
                    .key("add-row")
                    .padding_x(2)
                    .style("flex", "1 1 100%")
                    .with_child(
                        Button::new(tr!("Add Row"))
                            .class(css::ColorScheme::Secondary)
                            .icon_class("fa fa-plus-circle")
                            .on_activate(ctx.link().callback(|_| Msg::AddRow)),
                    ),
            );
        }
        view.add_child(row);
        view.into_html_with_ref(self.node_ref.clone())
    }
}

fn create_menu(ctx: &yew::Context<RowViewComp>, new_coords: Position) -> Menu {
    let create_callback = |widget: WidgetType| {
        ctx.link()
            .callback(move |_| Msg::AddWidget(new_coords, widget.clone()))
    };
    Menu::new()
        .with_item(
            MenuItem::new(tr!("Remote Panel"))
                .on_select(create_callback(WidgetType::Remotes { show_wizard: true })),
        )
        .with_item(
            MenuItem::new(tr!("Node Panels")).menu(
                Menu::new()
                    .with_item(
                        MenuItem::new(tr!("All Nodes"))
                            .on_select(create_callback(WidgetType::Nodes { remote_type: None })),
                    )
                    .with_item(MenuItem::new(tr!("PBS Nodes")).on_select(create_callback(
                        WidgetType::Nodes {
                            remote_type: Some(RemoteType::Pbs),
                        },
                    )))
                    .with_item(MenuItem::new(tr!("PVE Nodes")).on_select(create_callback(
                        WidgetType::Nodes {
                            remote_type: Some(RemoteType::Pve),
                        },
                    ))),
            ),
        )
        .with_item(
            MenuItem::new(tr!("Guest Panels")).menu(
                Menu::new()
                    .with_item(
                        MenuItem::new(tr!("All Guests"))
                            .on_select(create_callback(WidgetType::Guests { guest_type: None })),
                    )
                    .with_item(
                        MenuItem::new(tr!("Virtual Machines")).on_select(create_callback(
                            WidgetType::Guests {
                                guest_type: Some(crate::pve::GuestType::Qemu),
                            },
                        )),
                    )
                    .with_item(
                        MenuItem::new(tr!("Linux Container")).on_select(create_callback(
                            WidgetType::Guests {
                                guest_type: Some(crate::pve::GuestType::Lxc),
                            },
                        )),
                    ),
            ),
        )
        .with_item(
            MenuItem::new(tr!("Subscription Panel"))
                .on_select(create_callback(WidgetType::Subscription)),
        )
        .with_item(
            MenuItem::new(tr!("PBS Datastores"))
                .on_select(create_callback(WidgetType::PbsDatastores)),
        )
        .with_item(
            MenuItem::new(tr!("Leaderboards")).menu(
                Menu::new()
                    .with_item(
                        MenuItem::new(tr!("Guests with Highest CPU Usage")).on_select(
                            create_callback(WidgetType::Leaderboard {
                                leaderboard_type:
                                    crate::dashboard::types::LeaderboardType::GuestCpu,
                            }),
                        ),
                    )
                    .with_item(
                        MenuItem::new(tr!("Nodes With the Hightest CPU Usagge)")).on_select(
                            create_callback(WidgetType::Leaderboard {
                                leaderboard_type: crate::dashboard::types::LeaderboardType::NodeCpu,
                            }),
                        ),
                    )
                    .with_item(
                        MenuItem::new(tr!("Nodes With the Highest Memory Usage")).on_select(
                            create_callback(WidgetType::Leaderboard {
                                leaderboard_type:
                                    crate::dashboard::types::LeaderboardType::NodeMemory,
                            }),
                        ),
                    ),
            ),
        )
        .with_item(
            MenuItem::new(tr!("Task Summaries")).menu(
                Menu::new()
                    .with_item(MenuItem::new(tr!("Task Summary by Category")).on_select(
                        create_callback(WidgetType::TaskSummary {
                            grouping: crate::dashboard::types::TaskSummaryGrouping::Category,
                        }),
                    ))
                    .with_item(
                        MenuItem::new(tr!("Task Summary Sorted by Failed Tasks")).on_select(
                            create_callback(WidgetType::TaskSummary {
                                grouping: crate::dashboard::types::TaskSummaryGrouping::Remote,
                            }),
                        ),
                    ),
            ),
        )
        .with_item(MenuItem::new(tr!("SDN Panel")).on_select(create_callback(WidgetType::Sdn)))
}
