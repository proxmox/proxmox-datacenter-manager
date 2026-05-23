//! Shared rendering helpers for the Ceph UI.

use pwt::css::FontColor;
use pwt::prelude::*;
use pwt::widget::Container;

use proxmox_yew_comp::Status;

use pdm_api_types::ceph::CephClusterState;

/// Threshold color for a usage percentage (0-100): red at 85%+, amber at 70%+,
/// and none below (so a healthy value renders as plain text, not link-blue).
pub fn usage_color(pct: f64) -> Option<FontColor> {
    if pct >= 85.0 {
        Some(FontColor::Error)
    } else if pct >= 70.0 {
        Some(FontColor::Warning)
    } else {
        None
    }
}

/// Render `text` colored by the usage threshold, neutral below 70%.
pub fn usage_cell(text: String, pct: f64) -> Html {
    match usage_color(pct) {
        Some(color) => Container::new().class(color).with_child(text).into(),
        None => html! { { text } },
    }
}

/// Convenience for a plain "X.X%" usage cell.
pub fn usage_pct_cell(pct: f64) -> Html {
    usage_cell(format!("{pct:.1}%"), pct)
}

/// Map a Ceph health string (`HEALTH_OK` / `HEALTH_WARN` / `HEALTH_ERR`) to a
/// status icon severity. Anything unknown or absent maps to [`Status::Unknown`].
pub fn ceph_health_status(health: Option<&str>) -> Status {
    match health {
        Some("HEALTH_OK") => Status::Success,
        Some("HEALTH_WARN") => Status::Warning,
        Some("HEALTH_ERR") => Status::Error,
        _ => Status::Unknown,
    }
}

/// Humanized, translatable health label. The severity icon already conveys the
/// state, so this gives a plain word rather than the raw `HEALTH_*` token.
pub fn ceph_health_label(health: Option<&str>) -> String {
    match health {
        Some("HEALTH_OK") => tr!("OK"),
        Some("HEALTH_WARN") => tr!("Warning"),
        Some("HEALTH_ERR") => tr!("Error"),
        Some(other) if !other.is_empty() => other.to_string(),
        _ => tr!("Unknown"),
    }
}

/// Severity rank for sorting/triage: higher is worse. Unknown sorts above OK so
/// a cluster with no cached status is not mistaken for healthy.
pub fn ceph_health_severity(health: Option<&str>) -> u8 {
    match health {
        Some("HEALTH_ERR") => 3,
        Some("HEALTH_WARN") => 2,
        Some("HEALTH_OK") => 0,
        _ => 1,
    }
}

/// Human-readable, translatable label for a cluster's detection state.
pub fn ceph_cluster_state_label(state: CephClusterState) -> String {
    match state {
        CephClusterState::Detected => tr!("Detected"),
        CephClusterState::Unreachable => tr!("Unreachable"),
        CephClusterState::Gone => tr!("Gone"),
        CephClusterState::Tombstoned => tr!("Forgotten"),
    }
}
