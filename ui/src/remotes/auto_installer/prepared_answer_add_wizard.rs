//! Implements the configuration dialog UI for the auto-installer integration.

use anyhow::Result;
use js_sys::Intl;
use proxmox_installer_types::answer;
use std::{collections::BTreeMap, future::Future, pin::Pin, rc::Rc};
use wasm_bindgen::JsValue;
use yew::{
    html::IntoEventCallback,
    virtual_dom::{Key, VComp, VNode},
};

use pdm_api_types::auto_installer::{
    AnswerToken, AnswerTokenCreateResult, DiskSelectionMode, PreparedInstallationConfig,
    PreparedInstallationConfigCreateResult,
};
use proxmox_yew_comp::{
    LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState, Wizard, WizardPageRenderInfo,
};
use pwt::{prelude::*, state::Store, widget::TabBarItem};
use pwt_macros::builder;

use super::pdm_origin;
use super::prepared_answer_form::*;
use crate::pdm_client;

#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct AddAnswerWizardProperties {
    /// Dialog close callback.
    #[builder_cb(IntoEventCallback, into_event_callback, ())]
    #[prop_or_default]
    pub on_close: Option<Callback<()>>,

    /// Dialog submit results callback.
    #[builder_cb(IntoEventCallback, into_event_callback, (PreparedInstallationConfig, Option<AnswerTokenCreateResult>))]
    #[prop_or_default]
    pub on_submit_result:
        Option<Callback<(PreparedInstallationConfig, Option<AnswerTokenCreateResult>)>>,

    /// Auto-installer answer configuration.
    config: PreparedInstallationConfig,
}

impl AddAnswerWizardProperties {
    pub fn new() -> Self {
        let mut template_counters = BTreeMap::new();
        template_counters.insert("installation_nr".to_owned(), 0i32);

        let config = PreparedInstallationConfig {
            id: String::new(),
            authorized_tokens: Vec::new(),
            // target filter
            is_default: false,
            target_filter: BTreeMap::new(),
            // global options
            country: "at".to_owned(),
            fqdn: "{{product.product}}{{installation_nr}}.example.com".to_string(),
            use_dhcp_fqdn: false,
            keyboard: answer::KeyboardLayout::default(),
            mailto: String::new(),
            timezone: js_timezone().unwrap_or_else(|| "Etc/UTC".to_owned()),
            root_password_hashed: None,
            reboot_on_error: false,
            reboot_mode: answer::RebootMode::default(),
            root_ssh_keys: Vec::new(),
            // network options
            use_dhcp_network: true,
            cidr: None,
            gateway: None,
            dns: None,
            netdev_filter: BTreeMap::new(),
            netif_name_pinning_enabled: true,
            // disk options
            filesystem: answer::FilesystemOptions::Ext4(answer::LvmOptions::default()),
            disk_mode: DiskSelectionMode::default(),
            disk_list: Vec::new(),
            disk_filter: BTreeMap::new(),
            disk_filter_match: None,
            // post hook
            post_hook_cert_fp: None,
            post_hook_base_url: pdm_origin(),
            // templating
            template_counters,
            // subscription
            subscription_key: None,
        };

        yew::props!(Self { config })
    }

    pub fn with(config: PreparedInstallationConfig) -> Self {
        yew::props!(Self { config })
    }
}

impl From<AddAnswerWizardProperties> for VNode {
    fn from(value: AddAnswerWizardProperties) -> Self {
        let comp =
            VComp::new::<LoadableComponentMaster<AddAnswerWizardComponent>>(Rc::new(value), None);
        VNode::from(comp)
    }
}

struct AddAnswerWizardComponent {
    state: LoadableComponentState<()>,
    token_store: Store<AnswerToken>,
}

pwt::impl_deref_mut_property!(AddAnswerWizardComponent, state, LoadableComponentState<()>);

impl LoadableComponent for AddAnswerWizardComponent {
    type Properties = AddAnswerWizardProperties;
    type Message = ();
    type ViewState = ();

    fn create(_ctx: &LoadableComponentContext<Self>) -> Self {
        let store = Store::with_extract_key(|record: &AnswerToken| Key::from(record.id.to_owned()));
        store.set_sorter(|a: &AnswerToken, b: &AnswerToken| a.id.cmp(&b.id));

        Self {
            state: LoadableComponentState::new(),
            token_store: store,
        }
    }

    fn load(
        &self,
        _ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<()>>>> {
        let store = self.token_store.clone();
        Box::pin(async move {
            let data = pdm_client()
                .get_autoinst_tokens()
                .await?
                .into_iter()
                .collect();

            store.write().set_data(data);
            Ok(())
        })
    }

    fn main_view(&self, ctx: &LoadableComponentContext<Self>) -> Html {
        let props = ctx.props();
        let link = ctx.link().clone();

        Wizard::new(tr!("Add Prepared Answer"))
            .width(900)
            .resizable(true)
            .on_submit({
                let on_submit_result = props.on_submit_result.clone();
                move |config: serde_json::Value| {
                    let link = link.clone();
                    let on_submit_result = on_submit_result.clone();
                    async move {
                        match submit(config).await {
                            Ok(PreparedInstallationConfigCreateResult { config, token }) => {
                                if let Some(on_submit_result) = on_submit_result {
                                    on_submit_result.emit((config, token));
                                }
                            }
                            Err(err) => link.show_error(
                                tr!("Failed to create installation configuration"),
                                err,
                                true,
                            ),
                        }
                        Ok(())
                    }
                }
            })
            .on_close(props.on_close.clone())
            .with_page(TabBarItem::new().label(tr!("Global Options")), {
                let config = props.config.clone();
                move |_: &WizardPageRenderInfo| render_global_options_form(&config, true)
            })
            .with_page(TabBarItem::new().label(tr!("Network Options")), {
                let config = props.config.clone();
                move |p: &WizardPageRenderInfo| render_network_options_form(&p.form_ctx, &config)
            })
            .with_page(TabBarItem::new().label(tr!("Disk Setup")), {
                let config = props.config.clone();
                move |p: &WizardPageRenderInfo| render_disk_setup_form(&p.form_ctx, &config)
            })
            .with_page(TabBarItem::new().label(tr!("Target Filter")), {
                let config = props.config.clone();
                move |p: &WizardPageRenderInfo| render_target_filter_form(&p.form_ctx, &config)
            })
            .with_page(TabBarItem::new().label(tr!("Templating")), {
                let config = props.config.clone();
                move |_: &WizardPageRenderInfo| render_templating_form(&config)
            })
            .with_page(TabBarItem::new().label(tr!("Post Hook")), {
                let config = props.config.clone();
                let secrets = self.token_store.clone();
                move |p: &WizardPageRenderInfo| {
                    render_auth_form(&p.form_ctx, &config, secrets.clone())
                }
            })
            .into()
    }
}

impl AddAnswerWizardComponent {}

async fn submit(form_data: serde_json::Value) -> Result<PreparedInstallationConfigCreateResult> {
    let data = prepare_form_data(form_data)?;
    let root_password = data["root-password"].as_str().map(ToOwned::to_owned);

    Ok(pdm_client()
        .add_autoinst_prepared_answer(&serde_json::from_value(data)?, root_password.as_deref())
        .await?)
}

fn js_timezone() -> Option<String> {
    let datetime_options = Intl::DateTimeFormat::default().resolved_options();
    js_sys::Reflect::get(&datetime_options, &JsValue::from_str("timeZone"))
        .ok()
        .and_then(|v| v.as_string())
}
