use pwt::prelude::*;

use crate::widget::ContentSpacer;

mod webauthn;

#[function_component(OtherPanel)]
pub fn create_other_panel() -> Html {
    ContentSpacer::new()
        .class(pwt::css::FlexFit)
        .with_child(html! { <webauthn::WebauthnPanel/> })
        .into()
}
