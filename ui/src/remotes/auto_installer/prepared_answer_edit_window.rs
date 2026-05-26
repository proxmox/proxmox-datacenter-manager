//! Implements the configuration dialog UI for the auto-installer integration.

use anyhow::Result;
use std::{future::Future, pin::Pin, rc::Rc};
use yew::{
    html::IntoEventCallback,
    virtual_dom::{Key, VComp, VNode},
};

use crate::pdm_client;
use pdm_api_types::auto_installer::{
    AnswerToken, AnswerTokenCreateResult, DeletablePreparedInstallationConfigProperty,
    PreparedInstallationConfig, PreparedInstallationConfigUpdateResult,
};
use proxmox_yew_comp::{
    form::delete_empty_values, percent_encoding::percent_encode_component, EditWindow,
    LoadableComponent, LoadableComponentContext, LoadableComponentMaster,
    LoadableComponentScopeExt, LoadableComponentState,
};
use pwt::{
    css::FlexFit,
    prelude::*,
    state::Store,
    widget::{form::FormContext, TabBarItem, TabPanel},
};
use pwt_macros::builder;

use super::prepared_answer_form::*;

#[derive(Clone, PartialEq, Properties)]
#[builder]
pub struct EditAnswerWindowProperties {
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

impl EditAnswerWindowProperties {
    pub fn new(config: PreparedInstallationConfig) -> Self {
        yew::props!(Self { config })
    }
}

impl From<EditAnswerWindowProperties> for VNode {
    fn from(value: EditAnswerWindowProperties) -> Self {
        let comp =
            VComp::new::<LoadableComponentMaster<EditAnswerWindowComponent>>(Rc::new(value), None);
        VNode::from(comp)
    }
}

struct EditAnswerWindowComponent {
    state: LoadableComponentState<()>,
    token_store: Store<AnswerToken>,
}

pwt::impl_deref_mut_property!(EditAnswerWindowComponent, state, LoadableComponentState<()>);

impl LoadableComponent for EditAnswerWindowComponent {
    type Properties = EditAnswerWindowProperties;
    type Message = ();
    type ViewState = ();

    fn create(_ctx: &LoadableComponentContext<Self>) -> Self {
        let token_store =
            Store::with_extract_key(|record: &AnswerToken| Key::from(record.id.to_owned()));
        token_store.set_sorter(|a: &AnswerToken, b: &AnswerToken| a.id.cmp(&b.id));

        Self {
            state: LoadableComponentState::new(),
            token_store,
        }
    }

    fn load(
        &self,
        _ctx: &LoadableComponentContext<Self>,
    ) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>>>> {
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

        EditWindow::new(tr!("Edit Prepared Answer"))
            .width(900)
            .resizable(true)
            .renderer({
                let props = props.clone();
                let token_store = self.token_store.clone();
                move |form_ctx: &FormContext| render_tabpanel(form_ctx, &props, token_store.clone())
            })
            .edit(true)
            .submit_digest(true)
            .on_submit({
                let id = props.config.id.clone();
                let on_submit_result = props.on_submit_result.clone();
                move |form_ctx: FormContext| {
                    let id = id.clone();
                    let link = link.clone();
                    let on_submit_result = on_submit_result.clone();
                    let config = form_ctx.get_submit_data();

                    async move {
                        match submit(&percent_encode_component(&id), config).await {
                            Ok(PreparedInstallationConfigUpdateResult { config, token }) => {
                                if let Some(on_submit_result) = on_submit_result {
                                    on_submit_result.emit((config, token));
                                }
                            }
                            Err(err) => link.show_error(
                                tr!("Failed to update installation configuration"),
                                err,
                                true,
                            ),
                        }
                        Ok(())
                    }
                }
            })
            .on_close(props.on_close.clone())
            .advanced_checkbox(true)
            .into()
    }
}

async fn submit(
    id: &str,
    form_data: serde_json::Value,
) -> Result<PreparedInstallationConfigUpdateResult> {
    let data = delete_empty_values(
        &prepare_form_data(form_data)?,
        &[
            "root-ssh-keys",
            "post-hook-base-url",
            "post-hook-cert-fp",
            "disk-filter",
            "netdev-filter",
            "target-filter",
            "subscription-key",
            "template-counters",
        ],
        true,
    );

    let root_password = data["root-password"].as_str().map(ToOwned::to_owned);
    let delete = data["delete"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .flat_map(|s| s.as_str().and_then(|s| s.parse().ok()))
        .collect::<Vec<DeletablePreparedInstallationConfigProperty>>();

    Ok(pdm_client()
        .update_autoinst_prepared_answer(
            id,
            &serde_json::from_value(data)?,
            root_password.as_deref(),
            &delete,
        )
        .await?)
}

fn render_tabpanel(
    form_ctx: &FormContext,
    props: &EditAnswerWindowProperties,
    tokens: Store<AnswerToken>,
) -> yew::Html {
    TabPanel::new()
        .class(FlexFit)
        .force_render_all(true)
        .with_item(
            TabBarItem::new().label(tr!("Global Options")),
            render_global_options_form(&props.config, false),
        )
        .with_item(
            TabBarItem::new().label(tr!("Network Options")),
            render_network_options_form(form_ctx, &props.config),
        )
        .with_item(
            TabBarItem::new().label(tr!("Disk Setup")),
            render_disk_setup_form(form_ctx, &props.config),
        )
        .with_item(
            TabBarItem::new().label(tr!("Target Filter")),
            render_target_filter_form(form_ctx, &props.config),
        )
        .with_item(
            TabBarItem::new().label(tr!("Templating")),
            render_templating_form(&props.config),
        )
        .with_item(
            TabBarItem::new().label(tr!("Authentication")),
            render_auth_form(form_ctx, &props.config, tokens),
        )
        .into()
}
