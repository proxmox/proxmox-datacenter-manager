use std::rc::Rc;

use yew::html::IntoEventCallback;
use yew::prelude::*;
use yew::virtual_dom::{VComp, VNode};

use pwt::prelude::*;
use pwt::widget::form::Checkbox;
use pwt::widget::{Column, ConfirmDialog};

use pwt_macros::builder;

#[derive(PartialEq, Properties)]
#[builder]
pub struct RemoveRemote {
    /// A callback for an action that needs to be confirmed by the user.
    #[prop_or_default]
    #[builder_cb(IntoEventCallback, into_event_callback, bool)]
    pub on_confirm: Option<Callback<bool>>,

    /// A callback that will trigger if the user dismisses the dialog.
    #[prop_or_default]
    #[builder_cb(IntoEventCallback, into_event_callback, ())]
    pub on_dismiss: Option<Callback<()>>,
}

impl Default for RemoveRemote {
    fn default() -> Self {
        Self::new()
    }
}

impl RemoveRemote {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

enum Msg {
    SelectCheckBox(bool),
}

struct PdmRemoveRemote {
    keep_api_token: bool,
}

impl Component for PdmRemoveRemote {
    type Message = Msg;
    type Properties = RemoveRemote;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {
            keep_api_token: false,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::SelectCheckBox(v) => {
                self.keep_api_token = v;
                true
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        let keep_api_token = self.keep_api_token;

        let on_confirm = props.on_confirm.clone();
        let on_dismiss = props.on_dismiss.clone();

        let content = Column::new()
            .gap(2)
            .with_child(tr!("Are you sure you want to remove this remote?"))
            .with_child(
                Checkbox::new()
                    .box_label(tr!("Keep the API token on the remote"))
                    .checked(keep_api_token)
                    .on_change(ctx.link().callback(Msg::SelectCheckBox)),
            );

        let mut dialog = ConfirmDialog::default()
            .on_confirm(Callback::from(move |_| {
                if let Some(on_confirm) = &on_confirm {
                    on_confirm.emit(!keep_api_token);
                }
            }))
            .on_close(on_dismiss.clone())
            .on_dismiss(on_dismiss);

        dialog.set_confirm_message(content);

        dialog.into()
    }
}

impl From<RemoveRemote> for VNode {
    fn from(val: RemoveRemote) -> Self {
        let comp = VComp::new::<PdmRemoveRemote>(Rc::new(val), None);
        VNode::from(comp)
    }
}
