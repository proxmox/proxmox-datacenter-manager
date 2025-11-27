mod columns;
mod tree;
mod types;
mod ui_helpers;

// Re-export public types
pub use tree::FirewallTreeComponent;

use std::rc::Rc;
use yew::virtual_dom::{VComp, VNode};
use yew::Properties;

use proxmox_yew_comp::LoadableComponentMaster;

#[derive(PartialEq, Properties)]
pub struct FirewallTree {}

impl FirewallTree {
    pub fn new() -> Self {
        yew::props!(Self {})
    }
}

impl From<FirewallTree> for VNode {
    fn from(value: FirewallTree) -> Self {
        let comp =
            VComp::new::<LoadableComponentMaster<FirewallTreeComponent>>(Rc::new(value), None);
        VNode::from(comp)
    }
}
