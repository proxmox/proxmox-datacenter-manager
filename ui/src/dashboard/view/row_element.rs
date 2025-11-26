use yew::html::IntoEventCallback;

use pwt::css;
use pwt::prelude::*;
use pwt::props::RenderFn;
use pwt::widget::{ActionIcon, Card, Fa, Panel, Row};
use pwt_macros::{builder, widget};

use pdm_api_types::views::RowWidget;

#[widget(comp=RowElementComp, @element)]
#[derive(PartialEq, Properties, Clone)]
#[builder]
pub struct RowElement {
    item: RowWidget,
    widget_renderer: RenderFn<RowWidget>,

    #[builder]
    #[prop_or_default]
    edit_mode: bool,

    #[builder]
    #[prop_or_default]
    is_dragging: bool,

    #[builder_cb(IntoEventCallback, into_event_callback, ())]
    #[prop_or_default]
    on_remove: Option<Callback<()>>,

    #[builder_cb(IntoEventCallback, into_event_callback, u32)]
    #[prop_or_default]
    on_flex_change: Option<Callback<u32>>,
}

impl RowElement {
    pub fn new(item: RowWidget, widget_renderer: impl Into<RenderFn<RowWidget>>) -> Self {
        let widget_renderer = widget_renderer.into();
        yew::props!(Self {
            item,
            widget_renderer
        })
    }
}

pub enum Msg {
    FlexReduce,
    FlexIncrease,
}

pub struct RowElementComp {}

impl Component for RowElementComp {
    type Message = Msg;
    type Properties = RowElement;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {}
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        let props = ctx.props();
        let flex = props.item.flex.unwrap_or(1.0) as u32;
        match msg {
            Msg::FlexReduce => {
                if let Some(on_flex_change) = &props.on_flex_change {
                    on_flex_change.emit(flex.saturating_sub(1))
                }
            }
            Msg::FlexIncrease => {
                if let Some(on_flex_change) = &props.on_flex_change {
                    on_flex_change.emit(flex.saturating_add(1))
                }
            }
        }

        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let widget = props.widget_renderer.apply(&props.item);

        let edit_overlay = Card::new()
            .padding(2)
            .style("z-index", "10")
            .class(css::AlignItems::Center)
            .with_child(Fa::new("bars").style("cursor", "grab").padding_end(1))
            .with_child(tr!("Flex"))
            .with_child(
                ActionIcon::new("fa fa-minus")
                    .on_activate(ctx.link().callback(|_| Msg::FlexReduce)),
            )
            .with_child(props.item.flex.unwrap_or(1.0) as u32)
            .with_child(
                ActionIcon::new("fa fa-plus")
                    .on_activate(ctx.link().callback(|_| Msg::FlexIncrease)),
            )
            .with_child(ActionIcon::new("fa fa-times").on_activate({
                let on_remove = props.on_remove.clone();
                move |_| {
                    if let Some(on_remove) = &on_remove {
                        on_remove.emit(());
                    }
                }
            }));

        Panel::new()
            .with_std_props(&props.std_props)
            .listeners(&props.listeners)
            .border(true)
            .class(props.is_dragging.then_some("dragging-item"))
            .attribute("draggable", if props.edit_mode { "true" } else { "false" })
            .style("position", "relative")
            .with_child(widget)
            .with_optional_child(
                props.edit_mode.then_some(
                    Row::new()
                        .gap(2)
                        .class(css::Display::Flex)
                        .class(css::AlignItems::Start)
                        .class(css::JustifyContent::End)
                        .key("overlay")
                        .style("position", "absolute")
                        .style("inset", "0")
                        .with_child(edit_overlay),
                ),
            )
            .into()
    }
}
