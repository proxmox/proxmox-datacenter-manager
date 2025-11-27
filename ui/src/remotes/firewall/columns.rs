use proxmox_yew_comp::LoadableComponentContext;
use pwt::css::AlignItems;
use pwt::prelude::*;
use pwt::state::TreeStore;
use pwt::tr;
use pwt::widget::data_table::{DataTableColumn, DataTableHeader};
use pwt::widget::{Container, Fa, Row};
use std::rc::Rc;
use yew::Html;

use super::types::{Scope, TreeEntry, ViewState};
use super::ui_helpers::{
    render_firewall_status, render_load_error_message, render_rule_stats, render_warning_icon,
};

use crate::remotes::firewall::tree::FirewallTreeComponent;

pub fn create_columns(
    ctx: &LoadableComponentContext<FirewallTreeComponent>,
    store: TreeStore<TreeEntry>,
    loading: bool,
    scope: &Scope,
) -> Rc<Vec<DataTableHeader<TreeEntry>>> {
    let scope = Rc::new(scope.clone());

    Rc::new(vec![
        create_name_column(store, loading, scope.clone()),
        create_enabled_column(scope.clone()),
        create_rules_column(scope),
        create_actions_column(ctx),
    ])
}

fn create_name_column(
    store: TreeStore<TreeEntry>,
    loading: bool,
    scope: Rc<Scope>,
) -> DataTableHeader<TreeEntry> {
    DataTableColumn::new(tr!("Name"))
        .width("250px")
        .tree_column(store)
        .render(move |entry: &TreeEntry| {
            let (icon, text) = match entry {
                TreeEntry::Root if loading => {
                    let loading_text = tr!("Loading {}...", scope.loading_description());
                    (
                        Container::from_tag("i").class("pwt-loading-icon"),
                        loading_text,
                    )
                }
                _ => {
                    let icon = entry.icon_name();
                    let text = entry.name();
                    (
                        if let Some(icon) = icon {
                            Container::new().with_child(Fa::new(icon))
                        } else {
                            Container::new()
                        },
                        text,
                    )
                }
            };
            Row::new()
                .class(AlignItems::Baseline)
                .gap(2)
                .with_child(icon)
                .with_child(text)
                .into()
        })
        .into()
}

fn create_enabled_column(scope: Rc<Scope>) -> DataTableHeader<TreeEntry> {
    DataTableColumn::new(tr!("Enabled"))
        .width("40px")
        .justify("center")
        .render(move |entry: &TreeEntry| match entry {
            TreeEntry::Root => Html::default(),
            TreeEntry::Remote(_) => {
                if let Some((status, masked)) = entry.firewall_status() {
                    render_firewall_status(status, masked)
                } else if matches!(scope.as_ref(), Scope::Node { .. }) {
                    Html::default()
                } else {
                    render_warning_icon()
                }
            }
            _ => {
                if let Some((status, masked)) = entry.firewall_status() {
                    render_firewall_status(status, masked)
                } else {
                    render_warning_icon()
                }
            }
        })
        .into()
}

fn create_rules_column(scope: Rc<Scope>) -> DataTableHeader<TreeEntry> {
    DataTableColumn::new(tr!("Rules"))
        .width("minmax(160px, 1fr)")
        .render(move |entry: &TreeEntry| match entry {
            TreeEntry::Root => Html::default(),
            TreeEntry::Remote(_) => {
                if let Some(rules) = entry.rule_stats() {
                    render_rule_stats(rules)
                } else if matches!(scope.as_ref(), Scope::Node { .. }) {
                    Html::default()
                } else {
                    render_load_error_message()
                }
            }
            _ => {
                if let Some(rules) = entry.rule_stats() {
                    render_rule_stats(rules)
                } else {
                    render_load_error_message()
                }
            }
        })
        .into()
}

fn create_actions_column(
    ctx: &LoadableComponentContext<FirewallTreeComponent>,
) -> DataTableHeader<TreeEntry> {
    let link = ctx.link().clone();

    DataTableColumn::new(tr!("Actions"))
        .width("50px")
        .justify("right")
        .render(move |entry: &TreeEntry| {
            if !entry.is_editable() {
                return Html::default();
            }

            let view_state = match ViewState::from_entry(entry) {
                Some(state) => state,
                None => return Html::default(),
            };

            let link_clone = link.clone();
            pwt::widget::Tooltip::new(pwt::widget::ActionIcon::new("fa fa-fw fa-cog").on_activate(
                move |_| {
                    link_clone.change_view(Some(view_state.clone()));
                },
            ))
            .tip(tr!("Edit Options"))
            .into()
        })
        .into()
}
