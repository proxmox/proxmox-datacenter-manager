use std::rc::Rc;

use pwt::props::{AsClassesMut, AsCssStylesMut, CssStyles};
use pwt::widget::Container;
use yew::prelude::*;
use yew::virtual_dom::{Key, VComp, VNode};

use pwt::prelude::*;

#[derive(Clone, PartialEq, Properties)]
pub struct ContentSpacer {
    /// The yew component key.
    #[prop_or_default]
    pub key: Option<Key>,

    #[prop_or_default]
    pub children: Vec<VNode>,

    /// CSS class of the container.
    #[prop_or_default]
    pub class: Classes,

    /// CSS style for the dialog window
    #[prop_or_default]
    pub styles: CssStyles,
}

impl ContentSpacer {
    pub fn new() -> Self {
        yew::props!(Self {})
    }

    /// Builder style method to set the yew `key` property.
    pub fn key(mut self, key: impl IntoOptionalKey) -> Self {
        self.key = key.into_optional_key();
        self
    }

    /// Builder style method to add a html class.
    pub fn class(mut self, class: impl Into<Classes>) -> Self {
        self.add_class(class);
        self
    }

    /// Method to add a html class.
    pub fn add_class(&mut self, class: impl Into<Classes>) {
        self.class.push(class);
    }
}

impl ContainerBuilder for ContentSpacer {
    fn as_children_mut(&mut self) -> &mut Vec<VNode> {
        &mut self.children
    }
}

impl AsClassesMut for ContentSpacer {
    fn as_classes_mut(&mut self) -> &mut yew::Classes {
        &mut self.class
    }
}

impl AsCssStylesMut for ContentSpacer {
    fn as_css_styles_mut(&mut self) -> &mut CssStyles {
        &mut self.styles
    }
}

impl WidgetStyleBuilder for ContentSpacer {}

pub struct ProxmoxContentSpacer {}

impl Component for ProxmoxContentSpacer {
    type Message = ();
    type Properties = ContentSpacer;

    fn create(_ctx: &Context<Self>) -> Self {
        Self {}
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        Container::new()
            .class("proxmox-content-spacer")
            .class((props.children.len() < 2).then(|| "proxmox-content-spacer-with-one-child"))
            .class(props.class.clone())
            .styles(props.styles.clone())
            .children(props.children.clone())
            .into()
    }
}

impl From<ContentSpacer> for VNode {
    fn from(val: ContentSpacer) -> Self {
        let key = val.key.clone();
        let comp = VComp::new::<ProxmoxContentSpacer>(Rc::new(val), key);
        VNode::from(comp)
    }
}
