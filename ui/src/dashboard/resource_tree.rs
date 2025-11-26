use pwt::css;
use pwt::prelude::*;
use pwt::props::{ContainerBuilder, WidgetBuilder, WidgetStyleBuilder};
use pwt::widget::form::Field;
use pwt::widget::Column;
use pwt::widget::Panel;
use pwt::widget::Row;
use pwt::widget::Toolbar;

use crate::widget::ResourceTree;

#[function_component]
fn ResourceTreeWithSearch() -> Html {
    let search = use_state(String::new);

    Column::new()
        .class(css::FlexFit)
        .with_child(
            Toolbar::new()
                .with_child(tr!("Search"))
                .with_child(Field::new().on_change({
                    let search = search.clone();
                    move |value| search.set(value)
                })),
        )
        .with_child(
            // use another flex layout with base width to work around the data tables dynamic
            // column size that does not decrease
            Row::new().class(css::FlexFit).with_child(
                ResourceTree::new()
                    .search_term(search.to_string())
                    .flex(1.0)
                    .width(250)
                    .height(500)
                    .class(css::FlexFit),
            ),
        )
        .into()
}

pub fn create_resource_tree() -> Panel {
    Panel::new()
        .class(css::FlexFit)
        .title(tr!("Resources"))
        .with_child(html! {<ResourceTreeWithSearch />})
}
