//! Provides all shared components for the prepared answer create wizard and the corresponding
//! edit window, as well as some utility to collect and prepare the form data for submission.

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, ops::Deref, rc::Rc, sync::LazyLock};

use pdm_api_types::{
    auto_installer::{
        AnswerToken, DiskSelectionMode, PreparedInstallationConfig,
        PREPARED_INSTALL_CONFIG_ID_SCHEMA, TEMPLATE_COUNTER_NAME_REGEX, UDEV_FILTER_KEY_REGEX,
    },
    DISK_LIST_SCHEMA, EMAIL_SCHEMA,
};
use proxmox_installer_types::{
    answer::{
        BtrfsCompressOption, BtrfsOptions, FilesystemOptions, FilesystemType, FilterMatch,
        KeyboardLayout, LvmOptions, RebootMode, ZfsChecksumOption, ZfsCompressOption, ZfsOptions,
        BTRFS_COMPRESS_OPTIONS, FILESYSTEM_TYPE_OPTIONS, ROOT_PASSWORD_SCHEMA,
        SUBSCRIPTION_KEY_SCHEMA, ZFS_CHECKSUM_OPTIONS, ZFS_COMPRESS_OPTIONS,
    },
    EMAIL_DEFAULT_PLACEHOLDER,
};
use proxmox_schema::api_types::{CIDR_SCHEMA, IP_SCHEMA};
use proxmox_yew_comp::{utils::copy_text_to_clipboard, KeyValueList, SchemaValidation};
use pwt::{
    css::{ColorScheme, Flex, FlexFit, Overflow},
    prelude::*,
    props::FieldStdProps,
    state::Store,
    widget::{
        form::{
            Checkbox, Combobox, DisplayField, Field, FormContext, InputType, Number, TextArea,
            ValidateFn,
        },
        Button, Column, Container, Dialog, Fa, FieldLabel, FieldPosition, InputPanel, Row, Tooltip,
    },
};

use crate::remotes::auto_installer::token_selector::TokenSelector;

pub fn prepare_form_data(mut value: serde_json::Value) -> Result<serde_json::Value> {
    let obj = value
        .as_object_mut()
        .ok_or_else(|| anyhow!("form data must always be an object"))?;

    let fs_opts = collect_fs_options(obj);
    let disk_list: Vec<String> = obj
        .remove("disk-list")
        .and_then(|s| {
            s.as_str()
                .map(|s| s.split(',').map(|s| s.trim().to_owned()).collect())
        })
        .unwrap_or_default();

    let root_ssh_keys = collect_lines_into_array(obj.remove("root-ssh-keys"));

    value["filesystem"] = json!(fs_opts);
    value["disk-list"] = json!(disk_list);
    value["root-ssh-keys"] = root_ssh_keys;

    Ok(value)
}

fn collect_fs_options(obj: &mut serde_json::Map<String, Value>) -> FilesystemOptions {
    let fs_type = obj
        .get("filesystem-type")
        .and_then(|s| s.as_str())
        .and_then(|s| s.parse::<FilesystemType>().ok())
        .unwrap_or_default();

    let lvm_options = LvmOptions {
        hdsize: obj.remove("hdsize").and_then(|v| v.as_f64()),
        swapsize: obj.remove("swapsize").and_then(|v| v.as_f64()),
        maxroot: obj.remove("maxroot").and_then(|v| v.as_f64()),
        maxvz: obj.remove("maxvz").and_then(|v| v.as_f64()),
        minfree: obj.remove("minfree").and_then(|v| v.as_f64()),
    };

    match fs_type {
        FilesystemType::Ext4 => FilesystemOptions::Ext4(lvm_options),
        FilesystemType::Xfs => FilesystemOptions::Xfs(lvm_options),
        FilesystemType::Zfs(level) => FilesystemOptions::Zfs(ZfsOptions {
            raid: Some(level),
            ashift: obj
                .remove("ashift")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            arc_max: obj
                .remove("ashift")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            checksum: obj
                .remove("checksum")
                .and_then(|v| v.as_str().map(ToOwned::to_owned))
                .and_then(|s| s.parse::<ZfsChecksumOption>().ok()),
            compress: obj
                .remove("checksum")
                .and_then(|v| v.as_str().map(ToOwned::to_owned))
                .and_then(|s| s.parse::<ZfsCompressOption>().ok()),
            copies: obj
                .remove("copies")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            hdsize: obj.remove("hdsize").and_then(|v| v.as_f64()),
        }),
        FilesystemType::Btrfs(level) => FilesystemOptions::Btrfs(BtrfsOptions {
            raid: Some(level),
            compress: obj
                .remove("checksum")
                .and_then(|v| v.as_str().map(ToOwned::to_owned))
                .and_then(|s| s.parse::<BtrfsCompressOption>().ok()),
            hdsize: obj.remove("hdsize").and_then(|v| v.as_f64()),
        }),
    }
}

fn collect_lines_into_array(value: Option<Value>) -> Value {
    value
        .and_then(|v| v.as_str().map(|s| s.to_owned()))
        .map(|s| {
            json!(s
                .split('\n')
                .filter(|s| !s.is_empty())
                .collect::<Vec<&str>>())
        })
        .unwrap_or(Value::Array(Vec::new()))
}

pub fn render_global_options_form(
    config: &PreparedInstallationConfig,
    is_create: bool,
) -> yew::Html {
    let mut panel = InputPanel::new()
        .class(Flex::Fill)
        .class(Overflow::Auto)
        .padding(4);

    if is_create {
        panel.add_field(
            tr!("Installation ID"),
            Field::new()
                .name("id")
                .value(config.id.clone())
                .schema(&PREPARED_INSTALL_CONFIG_ID_SCHEMA)
                .required(true),
        );
    } else {
        panel.add_field(
            tr!("Installation ID"),
            DisplayField::new().name("id").value(config.id.clone()),
        );
    }

    panel
        .with_field(
            tr!("Country"),
            Combobox::new()
                .name("country")
                .placeholder(tr!("Two-letter country code, e.g. at"))
                .items(Rc::new(
                    COUNTRY_INFO
                        .deref()
                        .keys()
                        .map(|s| s.as_str().into())
                        .collect(),
                ))
                .render_value(|v: &AttrValue| {
                    if let Some(s) = COUNTRY_INFO.deref().get(&v.to_string()) {
                        s.into()
                    } else {
                        v.into()
                    }
                })
                .value(config.country.clone())
                .filter(|item: &AttrValue, query: &str| {
                    let query = query.to_string().to_lowercase();
                    let item = item.to_string();

                    // match by country code
                    if item.starts_with(&query) {
                        return true;
                    }

                    // match by country (common) name
                    if let Some(name) = COUNTRY_INFO.deref().get(&item) {
                        return name.to_lowercase().contains(&query);
                    }

                    false
                })
                .autoselect_filter(true)
                .required(true),
        )
        .with_field(
            tr!("Timezone"),
            Field::new()
                .name("timezone")
                .value(config.timezone.clone())
                .placeholder(tr!("Timezone name, e.g. Europe/Vienna"))
                .required(true),
        )
        .with_field(
            tr!("Root Password"),
            Field::new()
                .name("root-password")
                .input_type(InputType::Password)
                .schema(&ROOT_PASSWORD_SCHEMA)
                .placeholder((!is_create).then(|| tr!("Keep current")))
                .required(is_create),
        )
        .with_field(
            tr!("Keyboard Layout"),
            Combobox::new()
                .name("keyboard")
                .items(Rc::new(
                    KEYBOARD_LAYOUTS
                        .iter()
                        .map(|l| serde_variant_name(l).expect("valid variant").into())
                        .collect(),
                ))
                .render_value(|v: &AttrValue| {
                    v.parse::<KeyboardLayout>()
                        .map(|v| v.human_name().to_owned())
                        .unwrap_or_default()
                        .into()
                })
                .value(serde_variant_name(config.keyboard))
                .filter(|item: &AttrValue, query: &str| {
                    let query = query.to_string().to_lowercase();
                    let item = item.to_string();

                    // match by keyboard layout code
                    if item.starts_with(&query) {
                        return true;
                    }

                    // match by keyboard layout human name
                    if let Ok(human_name) = item.parse::<KeyboardLayout>()
                        .map(|v| v.human_name().to_owned()) {
                        return human_name.to_lowercase().contains(&query);
                    }

                    false
                })
                .autoselect_filter(true)
                .required(true),
        )
        .with_field(
            tr!("Administrator Email Address"),
            Field::new()
                .name("mailto")
                .placeholder(EMAIL_DEFAULT_PLACEHOLDER.to_owned())
                .input_type(InputType::Email)
                .value(config.mailto.clone())
                .schema(&EMAIL_SCHEMA)
                .validate(|s: &String| {
                    if s.ends_with(".invalid") {
                        bail!(tr!("Invalid (default) email address"))
                    } else {
                        Ok(())
                    }
                })
                .required(true),
        )
        .with_field(
            tr!("Root SSH Public Keys"),
            TextArea::new()
                .name("root-ssh-keys")
                .class("pwt-w-100")
                .submit_empty(false)
                .attribute("rows", "3")
                .placeholder(tr!("One per line, usually begins with \"ssh-\", \"sk-ssh-\", \"ecdsa-\" or \"sk-ecdsa\""))
                .value(config.root_ssh_keys.join("\n")),
        )
        .with_field(
            tr!("Reboot on Error"),
            Checkbox::new().name("reboot-on-error"),
        )
        .with_field(
            tr!("Post-Installation Action"),
            Combobox::new()
                .name("reboot-mode")
                .items(Rc::new(
                    [RebootMode::Reboot, RebootMode::PowerOff]
                        .iter()
                        .map(|opt| serde_variant_name(opt).expect("valid variant").into())
                        .collect(),
                ))
                .render_value(|v: &AttrValue| match v.parse::<RebootMode>() {
                    Ok(RebootMode::Reboot) => tr!("Reboot").into(),
                    Ok(RebootMode::PowerOff) => tr!("Power off").into(),
                    _ => v.into(),
                })
                .value(serde_variant_name(config.reboot_mode))
                .required(true),
        )
        .with_field(
            tr!("Subscription Key"),
            Field::new()
                .name("subscription-key")
                .placeholder(tr!("Optional, e.g. pve1c-0123456789"))
                .value(config.subscription_key.clone().unwrap_or_default())
                .schema(&SUBSCRIPTION_KEY_SCHEMA),
        )
        .into()
}

pub fn render_network_options_form(
    form_ctx: &FormContext,
    config: &PreparedInstallationConfig,
) -> yew::Html {
    let use_dhcp_network = form_ctx
        .read()
        .get_field_value("use-dhcp-network")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let use_dhcp_fqdn = form_ctx
        .read()
        .get_field_value("use-dhcp-fqdn")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    InputPanel::new()
        .class(Flex::Fill)
        .class(Overflow::Auto)
        .padding(4)
        .show_advanced(form_ctx.get_show_advanced())
        .with_field(
            tr!("Use DHCP"),
            Checkbox::new()
                .name("use-dhcp-network")
                .default(config.use_dhcp_network),
        )
        .with_field(
            tr!("IP Address (CIDR)"),
            Field::new()
                .name("cidr")
                .placeholder(tr!("E.g. 192.168.0.100/24"))
                .value(config.cidr.clone())
                .validate(templated_schema_validate(&CIDR_SCHEMA))
                .disabled(use_dhcp_network)
                .required(!use_dhcp_network),
        )
        .with_field(
            tr!("Gateway Address"),
            Field::new()
                .name("gateway")
                .placeholder(tr!("E.g. 192.168.0.1"))
                .value(config.gateway.clone())
                .validate(templated_schema_validate(&IP_SCHEMA))
                .disabled(use_dhcp_network)
                .required(!use_dhcp_network),
        )
        .with_field(
            tr!("DNS Server Address"),
            Field::new()
                .name("dns")
                .placeholder(tr!("E.g. 192.168.0.254"))
                .value(config.dns.clone())
                .validate(templated_schema_validate(&IP_SCHEMA))
                .disabled(use_dhcp_network)
                .required(!use_dhcp_network),
        )
        .with_field(
            tr!("FQDN from DHCP"),
            Checkbox::new()
                .name("use-dhcp-fqdn")
                .default(config.use_dhcp_fqdn),
        )
        .with_field(
            tr!("Fully-Qualified Domain Name (FQDN)"),
            Field::new()
                .name("fqdn")
                .placeholder("{{product.product}}{{installation_nr}}.example.com")
                .value(config.fqdn.to_string())
                .disabled(use_dhcp_fqdn)
                .tip(tr!(
                    "Hostname and domain to set for the target installation. Allows templating."
                ))
                .validate(|s: &String| {
                    if s != "{{product.product}}{{installation_nr}}.example.com" {
                        Ok(())
                    } else {
                        Err(anyhow!("Please adapt the default FQDN template!"))
                    }
                })
                .required(!use_dhcp_fqdn),
        )
        .with_field(
            tr!("Pin Network Interfaces"),
            Checkbox::new()
                .name("netif-name-pinning-enabled")
                .default(config.netif_name_pinning_enabled),
        )
        .with_advanced_spacer()
        .with_advanced_field(
            tr!("Network Device Filters"),
            KeyValueList::new()
                .value(
                    config
                        .netdev_filter
                        .iter()
                        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                        .collect(),
                )
                .key_label(tr!("Property Name"))
                .value_label(tr!("Value To Match"))
                .key_placeholder(tr!("udev property name, e.g. ID_NET_DRIVER"))
                .value_renderer(render_udev_filter_value.into())
                .submit_validate(kv_list_to_udev_filter_map_validate)
                .submit_empty(false)
                .name("netdev-filter")
                .class(FlexFit)
                .disabled(use_dhcp_network)
                .required(!use_dhcp_network),
        )
        .into()
}

pub fn render_disk_setup_form(
    form_ctx: &FormContext,
    config: &PreparedInstallationConfig,
) -> yew::Html {
    let disk_mode = form_ctx
        .read()
        .get_field_value("disk-mode")
        .and_then(|v| v.as_str().and_then(|s| s.parse::<DiskSelectionMode>().ok()))
        .unwrap_or_default();

    let fs_type = form_ctx
        .read()
        .get_field_value("filesystem-type")
        .and_then(|v| v.as_str().and_then(|s| s.parse::<FilesystemType>().ok()))
        .unwrap_or_default();

    let mut panel = InputPanel::new()
        .class(Flex::Fill)
        .class(Overflow::Auto)
        .padding(4)
        .show_advanced(form_ctx.get_show_advanced())
        .with_field(
            tr!("Filesystem"),
            Combobox::new()
                .name("filesystem-type")
                .items(Rc::new(
                    FILESYSTEM_TYPE_OPTIONS
                        .iter()
                        .map(|opt| serde_variant_name(opt).expect("valid variant").into())
                        .collect(),
                ))
                .render_value(|v: &AttrValue| {
                    v.parse::<FilesystemType>()
                        .map(|v| v.to_string())
                        .unwrap_or_default()
                        .into()
                })
                .value(serde_variant_name(config.filesystem.to_type()))
                .required(true)
                .show_filter(false),
        )
        .with_right_field(
            tr!("Disk Selection Mode"),
            Combobox::new()
                .name("disk-mode")
                .with_item("fixed")
                .with_item("filter")
                .default("fixed")
                .render_value(|v: &AttrValue| match v.parse::<DiskSelectionMode>() {
                    Ok(DiskSelectionMode::Fixed) => tr!("Fixed List of Disk Names").into(),
                    Ok(DiskSelectionMode::Filter) => tr!("Dynamically by udev Filter").into(),
                    _ => v.into(),
                })
                .required(true)
                .value(serde_variant_name(config.disk_mode)),
        )
        .with_field(
            tr!("Disk Names"),
            Field::new()
                .name("disk-list")
                .placeholder("sda, sdb")
                .value(config.disk_list.join(", "))
                .schema(&DISK_LIST_SCHEMA)
                .disabled(disk_mode != DiskSelectionMode::Fixed)
                .required(disk_mode == DiskSelectionMode::Fixed),
        )
        .with_spacer()
        .with_field(
            tr!("Disk udev Filter Mode"),
            Combobox::new()
                .name("disk-filter-match")
                .items(Rc::new(
                    [FilterMatch::Any, FilterMatch::All]
                        .iter()
                        .map(|opt| serde_variant_name(opt).expect("valid variant").into())
                        .collect(),
                ))
                .render_value(|v: &AttrValue| match v.parse::<FilterMatch>() {
                    Ok(FilterMatch::Any) => tr!("Match Any Filter").into(),
                    Ok(FilterMatch::All) => tr!("Match All Filters").into(),
                    _ => v.into(),
                })
                .default(serde_variant_name(FilterMatch::default()))
                .value(config.disk_filter_match.and_then(serde_variant_name))
                .disabled(disk_mode != DiskSelectionMode::Filter),
        )
        .with_large_field(
            tr!("Disk udev Filters"),
            KeyValueList::new()
                .value(
                    config
                        .disk_filter
                        .iter()
                        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                        .collect(),
                )
                .key_label(tr!("Property Name"))
                .value_label(tr!("Value To Match"))
                .key_placeholder(tr!("udev Property Name, e.g. ID_MODEL"))
                .value_renderer(render_udev_filter_value.into())
                .submit_validate(kv_list_to_udev_filter_map_validate)
                .submit_empty(false)
                .name("disk-filter")
                .class(FlexFit)
                .disabled(disk_mode != DiskSelectionMode::Filter)
                // The auto-installer rejects an empty filter at apply time ("need either
                // disk-list or filter set"), so reject it in the form too rather than let the
                // user submit a config that will fail. Required only in Filter mode; in Fixed
                // mode the field is disabled and validation is skipped.
                .required(disk_mode == DiskSelectionMode::Filter),
        );

    let warning = match fs_type {
        FilesystemType::Zfs(_) => Some(
            tr!("ZFS is not compatible with hardware RAID controllers, for details see the documentation.")
        ),
        FilesystemType::Btrfs(_) => Some(tr!(
            "Btrfs integration is a technology preview and only available for Proxmox Virtual Environment installations."
        )),
        _ => None,
    };

    if let Some(text) = warning {
        panel.add_large_custom_child(
            Container::from_tag("span")
                .key("fs-warning")
                .class("pwt-mt-2 pwt-d-block pwt-color-warning")
                .with_child(Fa::new("exclamation-circle").class("fa-fw"))
                .with_child(text),
        );
    }

    panel.add_spacer(true);

    // Dispatch the advanced field set on the live `fs_type` read from the form context (already
    // re-read on every render via EditWindow's FormDataChange redraw), not on the static config:
    // switching filesystem must refresh the advanced panel in place. When the live kind still
    // matches the saved kind reuse the saved payload so unrelated edits survive; otherwise fall
    // back to defaults so the form starts the new kind from a clean slate.
    add_fs_advanced_form_fields(&mut panel, fs_type, &config.filesystem);
    panel.into()
}

fn add_fs_advanced_form_fields(
    panel: &mut InputPanel,
    fs_type: FilesystemType,
    fs_opts: &FilesystemOptions,
) {
    if fs_type.is_lvm() {
        let lvm = match fs_opts {
            FilesystemOptions::Ext4(opts) | FilesystemOptions::Xfs(opts) => *opts,
            _ => LvmOptions::default(),
        };
        add_lvm_advanced_form_fields(panel, &lvm);
    } else if matches!(fs_type, FilesystemType::Zfs(_)) {
        let zfs = match fs_opts {
            FilesystemOptions::Zfs(opts) => *opts,
            _ => ZfsOptions::default(),
        };
        add_zfs_advanced_form_fields(panel, &zfs);
    } else if fs_type.is_btrfs() {
        let btrfs = match fs_opts {
            FilesystemOptions::Btrfs(opts) => *opts,
            _ => BtrfsOptions::default(),
        };
        add_btrfs_advanced_form_fields(panel, &btrfs);
    }
}

fn add_lvm_advanced_form_fields(panel: &mut InputPanel, fs_opts: &LvmOptions) {
    panel.add_field_with_options(
        FieldPosition::Left,
        true,
        false,
        tr!("Harddisk Size To Use (GB)"),
        Number::new()
            .name("hdsize")
            .min(4.)
            .step(0.1)
            .submit_empty(false)
            .value(fs_opts.hdsize.map(|v| v.to_string())),
    );

    panel.add_field_with_options(
        FieldPosition::Left,
        true,
        false,
        tr!("Swap Size (GB)"),
        Number::new()
            .name("swapsize")
            .min(0.)
            .max(fs_opts.hdsize.map(|v| v / 2.))
            .step(0.1)
            .submit_empty(false)
            .value(fs_opts.swapsize.map(|v| v.to_string())),
    );
    panel.add_field_with_options(
        FieldPosition::Right,
        true,
        false,
        tr!("Maximum root Volume Size (GB)"),
        Number::new()
            .name("maxroot")
            .min(0.)
            .max(fs_opts.hdsize.map(|v| v / 2.))
            .step(0.1)
            .submit_empty(false)
            .value(fs_opts.maxroot.map(|v| v.to_string())),
    );
    panel.add_field_with_options(
        FieldPosition::Right,
        true,
        false,
        tr!("Maximum data Volume Size (GB)"),
        Number::new()
            .name("maxvz")
            .min(0.)
            .max(fs_opts.hdsize.map(|v| v / 2.))
            .step(0.1)
            .submit_empty(false)
            .value(fs_opts.maxvz.map(|v| v.to_string())),
    );
    panel.add_field_with_options(
        FieldPosition::Right,
        true,
        false,
        tr!("Minimum Free Space in LVM Volume Group (GB)"),
        Number::new()
            .name("minfree")
            .min(0.)
            .max(fs_opts.hdsize.map(|v| v / 2.))
            .step(0.1)
            .submit_empty(false)
            .value(fs_opts.minfree.map(|v| v.to_string())),
    );
}

fn add_zfs_advanced_form_fields(panel: &mut InputPanel, fs_opts: &ZfsOptions) {
    panel.add_field_with_options(
        FieldPosition::Left,
        true,
        false,
        "ashift",
        Number::<u64>::new()
            .name("ashift")
            .min(9)
            .max(16)
            .step(1)
            .submit_empty(false)
            .value(fs_opts.ashift.map(|v| v.to_string())),
    );
    panel.add_field_with_options(
        FieldPosition::Left,
        true,
        false,
        tr!("ARC Maximum Size (MiB)"),
        Number::new()
            .name("arc-max")
            .min(64.)
            .step(1.)
            .submit_empty(false)
            .value(fs_opts.arc_max.map(|v| v.to_string())),
    );
    panel.add_field_with_options(
        FieldPosition::Right,
        true,
        false,
        tr!("Checksumming Algorithm"),
        Combobox::new()
            .name("checksum")
            .items(Rc::new(
                ZFS_CHECKSUM_OPTIONS
                    .iter()
                    .map(|opt| serde_variant_name(opt).expect("valid variant").into())
                    .collect(),
            ))
            .render_value(|v: &AttrValue| {
                v.parse::<ZfsChecksumOption>()
                    .map(|v| v.to_string())
                    .unwrap_or_default()
                    .into()
            })
            .submit_empty(false)
            .value(fs_opts.checksum.map(|v| v.to_string())),
    );
    panel.add_field_with_options(
        FieldPosition::Right,
        true,
        false,
        tr!("Compression Algorithm"),
        Combobox::new()
            .name("compress")
            .items(Rc::new(
                ZFS_COMPRESS_OPTIONS
                    .iter()
                    .map(|opt| serde_variant_name(opt).expect("valid variant").into())
                    .collect(),
            ))
            .render_value(|v: &AttrValue| {
                v.parse::<ZfsCompressOption>()
                    .map(|v| v.to_string())
                    .unwrap_or_default()
                    .into()
            })
            .submit_empty(false)
            .value(fs_opts.compress.map(|v| v.to_string())),
    );
    panel.add_field_with_options(
        FieldPosition::Right,
        true,
        false,
        tr!("Copies"),
        Number::<u32>::new()
            .name("copies")
            .min(1)
            .max(3)
            .step(1)
            .submit_empty(false)
            .value(fs_opts.copies.map(|v| v.to_string())),
    );
}

fn add_btrfs_advanced_form_fields(panel: &mut InputPanel, fs_opts: &BtrfsOptions) {
    panel.add_field_with_options(
        FieldPosition::Right,
        true,
        false,
        tr!("Compression Algorithm"),
        Combobox::new()
            .name("compress")
            .items(Rc::new(
                BTRFS_COMPRESS_OPTIONS
                    .iter()
                    .map(|opt| serde_variant_name(opt).expect("valid variant").into())
                    .collect(),
            ))
            .render_value(|v: &AttrValue| {
                v.parse::<BtrfsCompressOption>()
                    .map(|v| v.to_string())
                    .unwrap_or_default()
                    .into()
            })
            .submit_empty(false)
            .value(fs_opts.compress.map(|v| v.to_string())),
    );
}

pub fn render_target_filter_form(
    form_ctx: &FormContext,
    config: &PreparedInstallationConfig,
) -> yew::Html {
    let is_default = form_ctx
        .read()
        .get_field_value("is-default")
        .and_then(|v| v.as_bool())
        .unwrap_or(config.is_default);

    let has_target_filters = form_ctx
        .read()
        .get_field_value("target-filter")
        .and_then(|v| v.as_array().map(|vec| !vec.is_empty()))
        .unwrap_or(false);

    let mut panel = InputPanel::new()
        .class(Flex::Fill)
        .class(Overflow::Auto)
        .padding(4);

    if !is_default && !has_target_filters {
        panel.add_large_custom_child(
            Container::from_tag("span")
                .key("unmatchable-answer-warning")
                .class("pwt-color-warning pwt-mb-2 pwt-d-block")
                .with_child(Fa::new("exclamation-circle").class("fa-fw"))
                .with_child(tr!(
                    "Not marked as default answer and target filter are empty, answer will never be matched."
                ))
        );
    }

    panel
        .with_field(
            tr!("Default Answer"),
            Checkbox::new()
                .name("is-default")
                .tip(tr!(
                    "If selected, this configuration will be used if no other matches."
                ))
                .default(config.is_default),
        )
        .with_spacer()
        .with_large_field(
            tr!("Target Filters"),
            KeyValueList::new()
                .value(
                    config
                        .target_filter
                        .iter()
                        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                        .collect(),
                )
                .key_label(tr!("JSON Pointer"))
                .value_label(tr!("Value To Match"))
                .key_placeholder("/json/pointer")
                .submit_validate(|v: &Vec<(String, Value)>| {
                    let map: BTreeMap<String, String> = v
                        .iter()
                        .map(|(k, v)| {
                            (
                                k.clone(),
                                v.as_str().map(ToOwned::to_owned).unwrap_or_default(),
                            )
                        })
                        .collect();
                    Ok(serde_json::to_value(map)?)
                })
                .submit_empty(true)
                .name("target-filter")
                .class(FlexFit)
                .disabled(is_default),
        )
        .with_right_custom_child(Container::new().key("rfc-6901-hint").with_child(html! {
            <span style="float: right;">
                {tr!("references RFC 6901" => "Target filter keys are JSON pointers according to")}
                {" "}
                <a href="https://www.rfc-editor.org/rfc/rfc6901" target="_blank">{"RFC 6901"}</a>
                {"."}
            </span>
        }))
        .into()
}

pub fn render_templating_form(config: &PreparedInstallationConfig) -> yew::Html {
    InputPanel::new()
        .class(Flex::Fill)
        .class(Overflow::Auto)
        .padding(4)
        .with_large_custom_child(
            Container::from_tag("p")
                .key("counter-info")
                .with_child(tr!(
                    "Numerical template counters can be used to provide unique values across installations."
                )),
        )
        .with_large_custom_child(
            Container::from_tag("p")
                .key("counters-hint")
                .class("pwt-mb-2")
                .with_child(tr!(
                    "Counters are automatically incremented each time an answer is served."
                )),
        )
        .with_large_custom_child(
            KeyValueList::new()
                .value(
                    config
                        .template_counters
                        .iter()
                        .map(|(k, v)| (k.clone(), Value::Number((*v).into())))
                        .collect(),
                )
                .value_label(tr!("Current Value"))
                .value_renderer(render_template_counter_value.into())
                .submit_validate(kv_list_to_template_counter_map_validate)
                .submit_empty(false)
                .name("template-counters")
                .class(FlexFit),
        )
        .into()
}

pub fn render_auth_form(
    form_ctx: &FormContext,
    config: &PreparedInstallationConfig,
    tokens: Store<AnswerToken>,
) -> yew::Html {
    let has_tokens_selected = form_ctx
        .read()
        .get_field_value("authorized-tokens")
        .and_then(|v| v.as_array().map(|vec| !vec.is_empty()))
        .unwrap_or(false);

    let mut panel = InputPanel::new()
        .class(Flex::Fill)
        .class(Overflow::Auto)
        .padding(4)
        .with_custom_child(
            Container::from_tag("span")
                .key("authorized-tokens-title")
                .class("pwt-font-title-medium")
                .with_child(tr!("Authorized Tokens")),
        )
        .with_large_custom_child(
            TokenSelector::new(tokens)
                .selected_keys(config.authorized_tokens.clone())
                .required(false)
                .submit_empty(true)
                .name("authorized-tokens"),
        );

    if !has_tokens_selected {
        panel.add_large_custom_child(
            Container::from_tag("p")
                .key("auth-token-auto-create")
                .class("pwt-color-warning")
                .with_child(Fa::new("exclamation-circle").class("fa-fw"))
                .with_child(tr!(
                    "No existing authorization token selected. A new one will be automatically created."
                ))
        );
    }

    panel
        .with_spacer()
        .with_large_field(
            tr!("Proxmox Datacenter Manager Base URL"),
            Field::new()
                .name("post-hook-base-url")
                .tip(tr!(
                    "Base URL this PDM instance is reachable from the target host"
                ))
                .tip(pdm_origin())
                .value(config.post_hook_base_url.clone()),
        )
        .with_large_field(
            tr!("SHA256 Certificate Fingerprint"),
            Field::new()
                .name("post-hook-cert-fp")
                .tip(tr!("Optional certificate fingerprint"))
                .value(config.post_hook_cert_fp.clone()),
        )
        .with_large_custom_child(
            Container::from_tag("p")
                .key("post-hook-hint")
                .class("pwt-mt-2 pwt-color-primary")
                .with_child(Fa::new("info-circle").class("fa-fw"))
                .with_child(tr!(
                    "Optional. If provided, status reporting will be enabled."
                )),
        )
        .into()
}

pub fn render_show_secret_dialog(
    config_id: Option<&str>,
    token: &AnswerToken,
    secret: &str,
    on_close: Callback<()>,
) -> Option<yew::Html> {
    let token = format!("{}:{secret}", token.id);

    let copy_token_view = Container::new()
        .class("pwt-form-grid-col4")
        .with_child(FieldLabel::new(tr!("Token")))
        .with_child(
            Row::new()
                .class("pwt-fill-grid-row")
                .gap(2)
                .with_child(
                    Field::new()
                        .input_type(InputType::Password)
                        .class(FlexFit)
                        .value(token.to_owned())
                        .read_only(true),
                )
                .with_child(
                    Tooltip::new(
                        Button::new_icon("fa fa-clipboard")
                            .class(ColorScheme::Primary)
                            .on_activate({
                                let token = token.to_owned();
                                move |_| copy_text_to_clipboard(&token)
                            }),
                    )
                    .tip(tr!("Copy answer token to clipboard.")),
                ),
        );

    let answer_url = format!(
        "{}/api2/json/auto-install/answer",
        pdm_origin().unwrap_or_else(|| "https://pdm.example.com:8443".to_owned())
    );
    let commandline = format!(
        "proxmox-auto-install-assistant prepare-iso --fetch-from http --url {answer_url} --answer-auth-token {token} INPUT.iso",
    );

    let copy_commandline_view = Container::new()
        .class("pwt-form-grid-col4")
        .with_child(FieldLabel::new(tr!("Command Line")))
        .with_child(
            Row::new()
                .class("pwt-fill-grid-row")
                .gap(2)
                .with_child(
                    Field::new()
                        .input_type(InputType::Password)
                        .class(FlexFit)
                        .style("height", "2em")
                        .value(commandline.to_owned())
                        .read_only(true),
                )
                .with_child(
                    Tooltip::new(
                        Button::new_icon("fa fa-clipboard")
                            .class(ColorScheme::Primary)
                            .on_activate({
                                let commandline = commandline.to_owned();
                                move |_| copy_text_to_clipboard(&commandline)
                            }),
                    )
                    .tip(tr!("Copy template command line to clipboard. Replace INPUT.iso with your installation ISO.")),
                ),
        );

    let mut panel = InputPanel::new().padding(4);

    if let Some(id) = config_id {
        panel.add_large_field(
            false,
            false,
            tr!("Configuration ID"),
            DisplayField::new().value(id.to_owned()).read_only(true),
        );
    }

    panel.add_large_custom_child(copy_token_view);
    panel.add_large_custom_child(copy_commandline_view);

    let dialog = Dialog::new(tr!("New Answer Token")).on_close(on_close).with_child(Column::new().with_child(panel))
        .with_child(
            Container::new()
                .padding(4)
                .class(FlexFit)
                .class(ColorScheme::WarningContainer)
                .class("pwt-default-colors")
                .with_child(tr!(
                    "Please record the configuration token or ISO preparation command line - it will only be displayed once."
                )),
        );

    Some(dialog.into())
}

fn render_udev_filter_value(
    (_key, value, props, on_change): &(String, Value, FieldStdProps, Callback<String>),
) -> yew::Html {
    Field::new()
        .placeholder(tr!("glob to match"))
        .disabled(props.disabled)
        .value(value.as_str().map(|s| s.to_owned()).unwrap_or_default())
        .on_change(on_change)
        .into()
}

fn render_template_counter_value(
    (_key, value, props, on_change): &(String, Value, FieldStdProps, Callback<String>),
) -> yew::Html {
    Number::<i32>::new()
        .value(value.as_i64().unwrap_or_default().to_string())
        .disabled(props.disabled)
        .on_change({
            let on_change = on_change.clone();
            move |v: Option<Result<i32, String>>| {
                if let Some(Ok(v)) = v {
                    on_change.emit(v.to_string());
                }
            }
        })
        .into()
}

/// Validate against a schema, but accept any value carrying a MiniJinja placeholder, as the
/// backend substitutes those before parsing the field.
fn templated_schema_validate(schema: &'static proxmox_schema::Schema) -> ValidateFn<String> {
    ValidateFn::new(move |value: &String| {
        if contains_minijinja_variable(value) {
            return Ok(());
        }
        schema.parse_simple_value(value)?;
        Ok(())
    })
}

/// Whether the value contains a MiniJinja variable expression, that is `{{`, an identifier
/// (optionally dotted, like `product.product`), and `}}`, with optional surrounding whitespace.
fn contains_minijinja_variable(value: &str) -> bool {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '.';

    let mut rest = value;
    while let Some(open) = rest.find("{{") {
        let inner = &rest[open + 2..];
        if let Some(close) = inner.find("}}") {
            if inner[..close].trim().chars().all(is_ident) && !inner[..close].trim().is_empty() {
                return true;
            }
            rest = &inner[close + 2..];
        } else {
            break;
        }
    }
    false
}

#[allow(clippy::ptr_arg)]
fn kv_list_to_udev_filter_map_validate(v: &Vec<(String, Value)>) -> Result<Value> {
    let mut map = BTreeMap::<String, String>::new();
    for (k, v) in v {
        if UDEV_FILTER_KEY_REGEX.is_match(k) {
            map.insert(k.clone(), v.as_str().unwrap_or_default().to_owned());
        } else {
            bail!("udev property names must only consist of uppercase characters and underscores: {k}");
        }
    }

    Ok(serde_json::to_value(map)?)
}

#[allow(clippy::ptr_arg)]
fn kv_list_to_template_counter_map_validate(v: &Vec<(String, Value)>) -> Result<Value> {
    let mut map = BTreeMap::<String, i32>::new();
    for (k, v) in v {
        if TEMPLATE_COUNTER_NAME_REGEX.is_match(k) {
            let value = match v {
                Value::Number(number) => number.as_i64(),
                Value::String(text) if text == "" => Some(0),
                Value::String(text) => text.parse().ok(),
                _ => None,
            };
            match value.and_then(|v| v.try_into().ok()) {
                Some(v) => {
                    map.insert(k.clone(), v);
                }
                None => bail!("invalid value: {v}"),
            }
        } else {
            bail!("must be a valid minijinja identifier: {k}");
        }
    }

    Ok(serde_json::to_value(map)?)
}

fn serde_variant_name<T: Serialize>(ty: T) -> Option<String> {
    match serde_json::to_value(ty) {
        Ok(Value::String(s)) => Some(s),
        other => {
            log::warn!(
                "expected string of type {}, got {other:?}",
                std::any::type_name::<T>()
            );
            None
        }
    }
}

fn pdm_origin() -> Option<String> {
    gloo_utils::document()
        .url()
        .and_then(|s| web_sys::Url::new(&s))
        .map(|url| url.origin())
        .ok()
}

const KEYBOARD_LAYOUTS: &[KeyboardLayout] = {
    use KeyboardLayout::*;
    &[
        De, DeCh, Dk, EnGb, EnUs, Es, Fi, Fr, FrBe, FrCa, FrCh, Hu, Is, It, Jp, Lt, Mk, Nl, No, Pl,
        Pt, PtBr, Se, Si, Tr,
    ]
};

static COUNTRY_INFO: LazyLock<BTreeMap<String, String>> = LazyLock::new(|| {
    #[derive(Deserialize)]
    struct Iso3611CountryInfo {
        alpha_2: String,
        common_name: Option<String>,
        name: String,
    }

    #[derive(Deserialize)]
    struct Iso3611Info {
        #[serde(rename = "3166-1")]
        list: Vec<Iso3611CountryInfo>,
    }

    let raw: Iso3611Info =
        serde_json::from_str(include_str!("/usr/share/iso-codes/json/iso_3166-1.json"))
            .expect("valid country-info json");

    raw.list
        .into_iter()
        .map(|c| (c.alpha_2.to_lowercase(), c.common_name.unwrap_or(c.name)))
        .collect()
});
