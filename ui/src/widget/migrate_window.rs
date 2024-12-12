use anyhow::{bail, Error};
use serde_json::{json, Value};
use yew::{html::IntoEventCallback, Callback, Component, Properties};

use proxmox_client::ApiResponseData;
use proxmox_yew_comp::EditWindow;
use pwt::css;
use pwt::prelude::*;
use pwt::widget::{
    form::{Checkbox, DisplayField, FormContext, Number},
    Container, InputPanel,
};
use pwt_macros::{builder, widget};

use pdm_api_types::remotes::RemoteType;
use pdm_api_types::RemoteUpid;
use pdm_client::{MigrateLxc, MigrateQemu, RemoteMigrateLxc, RemoteMigrateQemu};

use crate::pve::GuestInfo;

use super::{
    PveMigrateMap, PveNetworkSelector, PveNodeSelector, PveStorageSelector, RemoteSelector,
};

#[widget(comp=PdmMigrateWindow)]
#[builder]
#[derive(Clone, Properties, PartialEq)]
/// The interactive window to start a migration for a single guest
pub struct MigrateWindow {
    /// The source remote of the guest
    pub remote: AttrValue,

    /// The guest Info
    pub guest_info: GuestInfo,

    /// Close/Abort callback.
    #[builder_cb(IntoEventCallback, into_event_callback, ())]
    #[prop_or_default]
    pub on_close: Option<Callback<()>>,

    /// Submit callback.
    ///
    /// Will be called when the window was successfully submitted.
    #[prop_or_default]
    #[builder_cb(IntoEventCallback, into_event_callback, RemoteUpid)]
    pub on_submit: Option<Callback<RemoteUpid>>,
}

impl MigrateWindow {
    pub fn new(remote: impl Into<AttrValue>, guest_info: GuestInfo) -> Self {
        yew::props!(Self {
            remote: remote.into(),
            guest_info,
        })
    }
}

pub enum Msg {
    RemoteChange(String),
    Result(RemoteUpid),
}

pub struct PdmMigrateWindow {
    target_remote: AttrValue,
}

impl PdmMigrateWindow {
    async fn load(
        remote: AttrValue,
        guest_info: GuestInfo,
    ) -> Result<ApiResponseData<serde_json::Value>, Error> {
        let mode = match guest_info.guest_type {
            crate::pve::GuestType::Qemu => {
                let status = crate::pdm_client()
                    .pve_qemu_status(&remote, None, guest_info.vmid)
                    .await?;

                match status.status {
                    pdm_client::types::IsRunning::Running => tr!("Online"),
                    pdm_client::types::IsRunning::Stopped => tr!("Offline"),
                }
            }
            crate::pve::GuestType::Lxc => {
                let status = crate::pdm_client()
                    .pve_lxc_status(&remote, None, guest_info.vmid)
                    .await?;
                match status.status {
                    pdm_client::types::IsRunning::Running => tr!("Restart"),
                    pdm_client::types::IsRunning::Stopped => tr!("Offline"),
                }
            }
        };

        let response = ApiResponseData {
            attribs: std::collections::HashMap::new(),
            data: json!({
                "migrate-mode": mode,
            }),
        };

        Ok(response)
    }

    async fn submit(
        scope: yew::html::Scope<Self>,
        remote: AttrValue,
        guest_info: GuestInfo,
        form_ctx: FormContext,
    ) -> Result<(), Error> {
        let value = form_ctx.get_submit_data();
        let target_remote = value["remote"].as_str().unwrap_or_default();

        let upid = if target_remote != remote {
            match guest_info.guest_type {
                crate::pve::GuestType::Qemu => {
                    let mut migrate_opts = RemoteMigrateQemu::new()
                        .delete_source(value["delete-source"].as_bool().unwrap_or_default())
                        .online(true);

                    if let Some(Value::Number(vmid)) = value.get("target-vmid") {
                        migrate_opts = migrate_opts.target_vmid(vmid.as_u64().unwrap() as u32);
                    }

                    if form_ctx.read().get_field_checked("detailed-mode") {
                        match value.get("detail-map") {
                            Some(Value::Array(list)) => {
                                for map in list {
                                    let (ty, mapping) = map
                                        .as_str()
                                        .unwrap_or_default()
                                        .split_once(":")
                                        .unwrap_or_default();
                                    let (from, to) = mapping.split_once("=").unwrap_or_default();

                                    log::error!("{from}={to}");
                                    match ty {
                                        "s" => migrate_opts = migrate_opts.map_storage(from, to),
                                        "n" => migrate_opts = migrate_opts.map_bridge(from, to),
                                        _ => {}
                                    }
                                }
                            }
                            _ => bail!("invalid map data"),
                        }
                    } else {
                        migrate_opts = migrate_opts
                            .map_storage("*", value["target_storage"].as_str().unwrap())
                            .map_bridge("*", value["target_network"].as_str().unwrap());
                    }
                    crate::pdm_client()
                        .pve_qemu_remote_migrate(
                            &remote,
                            None,
                            guest_info.vmid,
                            target_remote.to_string(),
                            migrate_opts,
                        )
                        .await?
                }
                crate::pve::GuestType::Lxc => {
                    let mut migrate_opts = RemoteMigrateLxc::new()
                        .delete_source(value["delete-source"].as_bool().unwrap_or_default())
                        .restart(true, None);

                    if form_ctx.read().get_field_checked("detailed-mode") {
                        match value.get("detail-map") {
                            Some(Value::Array(list)) => {
                                for map in list {
                                    let (ty, mapping) = map
                                        .as_str()
                                        .unwrap_or_default()
                                        .split_once(":")
                                        .unwrap_or_default();
                                    let (from, to) = mapping.split_once("=").unwrap_or_default();

                                    match ty {
                                        "s" => migrate_opts = migrate_opts.map_storage(from, to),
                                        "n" => migrate_opts = migrate_opts.map_bridge(from, to),
                                        _ => {}
                                    }
                                }
                            }
                            _ => bail!("invalid map data"),
                        }
                    } else {
                        migrate_opts = migrate_opts
                            .map_storage("*", value["target_storage"].as_str().unwrap())
                            .map_bridge("*", value["target_network"].as_str().unwrap());
                    }
                    crate::pdm_client()
                        .pve_lxc_remote_migrate(
                            &remote,
                            None,
                            guest_info.vmid,
                            target_remote.to_string(),
                            migrate_opts,
                        )
                        .await?
                }
            }
        } else {
            match guest_info.guest_type {
                crate::pve::GuestType::Qemu => {
                    crate::pdm_client()
                        .pve_qemu_migrate(
                            &remote,
                            None,
                            guest_info.vmid,
                            value["node"].as_str().unwrap().to_string(),
                            MigrateQemu::new().online(true),
                        )
                        .await?
                }
                crate::pve::GuestType::Lxc => {
                    crate::pdm_client()
                        .pve_lxc_migrate(
                            &remote,
                            None,
                            guest_info.vmid,
                            value["node"].as_str().unwrap().to_string(),
                            MigrateLxc::new().restart(true, None),
                        )
                        .await?
                }
            }
        };

        scope.send_message(Msg::Result(upid));
        Ok(())
    }

    fn input_panel(
        link: &yew::html::Scope<Self>,
        form_ctx: &FormContext,
        target_remote: AttrValue,
        source_remote: AttrValue,
        guest_info: GuestInfo,
    ) -> Html {
        let same_remote = target_remote == source_remote;
        if !same_remote {
            form_ctx.write().set_field_value("node", "".into());
        }
        let detail_mode = form_ctx.read().get_field_checked("detailed-mode");
        let mut input = InputPanel::new()
            .padding(4)
            // hidden field for migration status
            .with_field(
                tr!("Source Remote"),
                DisplayField::new(source_remote).key("source_remote"),
            )
            .with_right_field(
                tr!("Target Remote"),
                RemoteSelector::new()
                    .remote_type(RemoteType::Pve)
                    .name("remote")
                    .default(target_remote.clone())
                    .on_change(link.callback(Msg::RemoteChange))
                    .required(true),
            )
            .with_field(
                tr!("Mode"),
                DisplayField::new("").name("migrate-mode").key("mode"),
            )
            .with_right_field(
                tr!("Target Node"),
                PveNodeSelector::new(target_remote.clone())
                    .name("node")
                    .required(same_remote)
                    .disabled(!same_remote),
            );

        if !same_remote {
            input.add_spacer(false);
        }

        input.add_custom_child(
            Container::new()
                .key("remote_title")
                .class(same_remote.then_some(css::Display::None))
                .with_child(tr!("Remote Migration Settings")),
        );

        input.add_field_with_options(
            pwt::widget::FieldPosition::Left,
            false,
            same_remote,
            tr!("Delete Source"),
            Checkbox::new()
                .name("delete-source")
                .default(true)
                .disabled(same_remote),
        );
        input.add_field_with_options(
            pwt::widget::FieldPosition::Right,
            false,
            same_remote,
            tr!("Target VMID"),
            Number::new()
                .min(100u32)
                .max(999999999)
                .name("target-vmid")
                .placeholder(guest_info.vmid.to_string())
                .disabled(same_remote),
        );
        input.add_large_field(
            false,
            same_remote,
            tr!("Detailed Mapping"),
            Checkbox::new().name("detailed-mode"),
        );
        input.add_large_field(
            false,
            same_remote || detail_mode,
            tr!("Target Storage"),
            PveStorageSelector::new(target_remote.clone())
                .key(format!("storage-{target_remote}"))
                .name("target_storage")
                .disabled(detail_mode)
                .required(!detail_mode),
        );
        input.add_large_field(
            false,
            same_remote || detail_mode,
            tr!("Target Network"),
            PveNetworkSelector::new(target_remote.clone())
                .key(format!("network-{target_remote}"))
                .name("target_network")
                .disabled(detail_mode)
                .required(!detail_mode),
        );
        input.add_large_field(
            false,
            !detail_mode,
            "",
            PveMigrateMap::new(target_remote, guest_info)
                .name("detail-map")
                .submit(detail_mode)
                .required(detail_mode),
        );

        input.into()
    }
}

impl Component for PdmMigrateWindow {
    type Message = Msg;
    type Properties = MigrateWindow;

    fn create(ctx: &yew::Context<Self>) -> Self {
        Self {
            target_remote: ctx.props().remote.clone(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::RemoteChange(remote) => {
                let changed = self.target_remote != remote;
                self.target_remote = remote.into();
                changed
            }
            Msg::Result(remote_upid) => {
                if let Some(on_submit) = &ctx.props().on_submit {
                    on_submit.emit(remote_upid);
                }
                true
            }
        }
    }

    fn view(&self, ctx: &yew::Context<Self>) -> yew::Html {
        let props = ctx.props();
        let guest_info = props.guest_info;
        let remote = props.remote.clone();
        EditWindow::new(tr!("Migrate"))
            .submit_text(tr!("Migrate"))
            .on_close(props.on_close.clone())
            .on_submit({
                let link = ctx.link().clone();
                move |ctx| Self::submit(link.clone(), remote.clone(), guest_info, ctx)
            })
            .loader({
                let remote = props.remote.clone();
                move || Self::load(remote.clone(), guest_info)
            })
            .renderer({
                let target = self.target_remote.clone();
                let source_remote = ctx.props().remote.clone();
                let link = ctx.link().clone();
                move |form| {
                    Self::input_panel(
                        &link,
                        form,
                        target.clone(),
                        source_remote.clone(),
                        guest_info,
                    )
                }
            })
            .into()
    }
}