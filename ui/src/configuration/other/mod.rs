use pwt::prelude::*;
use pwt::widget::Column;

mod webauthn;

#[function_component(OtherPanel)]
pub fn create_other_panel() -> Html {
    Column::new()
        .class("pwt-flex-fill")
        .padding(2)
        .gap(4)
        .with_child(html! { <webauthn::WebauthnPanel/> })
        .into()
}
