use std::rc::Rc;

use yew::virtual_dom::{VComp, VNode};

use pwt::{
    css::FlexFit,
    prelude::*,
    widget::{
        form::{Checkbox, Field},
        InputPanel,
    },
};

use proxmox_yew_comp::WizardPageRenderInfo;

use pdm_api_types::{remotes::Remote, Authid};

use pwt_macros::builder;

#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct WizardPageInfo {
    info: WizardPageRenderInfo,

    #[builder]
    #[prop_or_default]
    server_info: Option<Remote>,
}

impl WizardPageInfo {
    pub fn new(info: WizardPageRenderInfo) -> Self {
        yew::props!(Self { info })
    }
}

pub struct PdmWizardPageInfo {
    create_token: bool,
}

pub enum Msg {
    ToggleCreateToken(bool),
}

impl Component for PdmWizardPageInfo {
    type Message = Msg;
    type Properties = WizardPageInfo;

    fn create(ctx: &Context<Self>) -> Self {
        let props = ctx.props();
        if props.server_info.is_none() {
            props.info.page_lock(true);
        }

        Self { create_token: true }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::ToggleCreateToken(create_token) => {
                self.create_token = create_token;
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let mut is_user = true;
        if let Some(Some(authid)) = ctx
            .props()
            .info
            .valid_data
            .get("authid")
            .map(|a| a.as_str())
        {
            match authid.parse::<Authid>() {
                Ok(authid) => is_user = !authid.is_token(),
                Err(_) => {}
            }
        }
        let name = ctx
            .props()
            .server_info
            .as_ref()
            .map(|s| s.id.to_string())
            .unwrap_or_default();
        InputPanel::new()
            .class(FlexFit)
            .padding(4)
            .with_field(
                tr!("Remote ID"),
                Field::new().default(name).name("id").required(true),
            )
            .with_field(
                tr!("Create Token"),
                Checkbox::new()
                    .key("create-token-cb")
                    .submit(false)
                    .disabled(is_user)
                    .default(self.create_token || is_user)
                    .on_change(ctx.link().callback(Msg::ToggleCreateToken)),
            )
            .with_field(
                tr!("Token Name"),
                Field::new()
                    .name("create-token")
                    .disabled(!self.create_token && !is_user)
                    .required(self.create_token || is_user)
                    .submit(self.create_token || is_user)
                    .default("pdm-admin"),
            )
            .into()
    }
}

impl From<WizardPageInfo> for VNode {
    fn from(val: WizardPageInfo) -> Self {
        let comp = VComp::new::<PdmWizardPageInfo>(Rc::new(val), None);
        VNode::from(comp)
    }
}
