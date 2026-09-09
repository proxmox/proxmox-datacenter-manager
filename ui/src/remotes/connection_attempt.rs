use gloo_timers::callback::Interval;
use http::StatusCode;
use pdm_api_types::remotes::REMOTE_ONBOARDING_TIMEOUT_SECS;
use pwt::css::AlignItems;
use pwt::prelude::*;
use pwt::widget::{Button, Column, Container, Row, error_message};
use yew::Callback;
use yew::virtual_dom::VNode;

/// A firewall that silently drops packets or a stalled service keeps the wizard waiting for the
/// whole deadline, so name the usual causes once the wait takes longer than usual.
const HINT_AFTER_SECS: u64 = 15;

/// Tracks one remote operation from a monotonic start instant.
pub struct ConnectionAttempt {
    started_at_millis: f64,
    elapsed_secs: u64,
    _interval: Interval,
}

impl ConnectionAttempt {
    pub fn new(on_tick: Callback<()>) -> Self {
        let interval = Interval::new(1_000, move || on_tick.emit(()));
        Self {
            started_at_millis: monotonic_now_millis(),
            elapsed_secs: 0,
            _interval: interval,
        }
    }

    pub(super) fn deadline_millis(&self) -> f64 {
        deadline_millis(self.started_at_millis)
    }

    pub fn tick(&mut self) -> bool {
        let elapsed_secs = elapsed_secs(self.started_at_millis, monotonic_now_millis());
        if self.elapsed_secs == elapsed_secs {
            false
        } else {
            self.elapsed_secs = elapsed_secs;
            true
        }
    }

    fn remaining_text(&self) -> String {
        countdown_text(self.elapsed_secs)
    }

    fn hint_text(&self) -> Option<String> {
        slow_hint(self.elapsed_secs)
    }
}

fn countdown_text(elapsed_secs: u64) -> String {
    let remaining = REMOTE_ONBOARDING_TIMEOUT_SECS.saturating_sub(elapsed_secs);
    tr!("(timeout in {n} second)" | "(timeout in {n} seconds)" % remaining)
}

fn slow_hint(elapsed_secs: u64) -> Option<String> {
    (elapsed_secs >= HINT_AFTER_SECS).then(|| {
        tr!("This is taking longer than usual. Check that the remote is reachable and not blocked by a firewall.")
    })
}

/// Masks `content` while an attempt runs and shows the operation, its countdown and a cancel button
/// in one panel. PWT's `Mask` only takes plain text, so this reuses its markup to fit the button
/// in, and keeps a live region mounted so screen readers announce the operation when it starts.
pub fn attempt_mask(
    content: impl Into<VNode>,
    attempt: Option<&ConnectionAttempt>,
    operation: &str,
    on_cancel: Callback<()>,
) -> Container {
    let mask = attempt.map(|attempt| {
        Container::new()
            .class("pwt-load-mask")
            .class("visible")
            .with_child(
                Container::new()
                    .class("pwt-load-mask-inner")
                    .with_child(Container::from_tag("i").class("pwt-loading-icon"))
                    .with_child(
                        Column::new()
                            .gap(1)
                            .with_child(
                                Row::new()
                                    .gap(1)
                                    .class(AlignItems::Center)
                                    .with_child(operation.to_string())
                                    // The countdown shows the time left until the deadline, not an
                                    // estimate of when the operation finishes. It changes every
                                    // second, so it stays out of the live region below to avoid
                                    // announcing it over and over.
                                    .with_child(attempt.remaining_text()),
                            )
                            .with_optional_child(attempt.hint_text()),
                    )
                    .with_child(
                        Button::new(tr!("Cancel")).on_activate(move |_| on_cancel.emit(())),
                    ),
            )
    });

    Container::new()
        .class("pwt-d-flex")
        .class("pwt-flex-fill-first-child")
        .class("pwt-position-relative")
        .with_child(
            Container::new()
                .class("pwt-d-flex")
                .class("pwt-flex-fill-first-child")
                // The overlay only blocks the pointer. Without `inert`, a focused field would still
                // take keyboard input that the running request ignores.
                .attribute("inert", attempt.is_some().then_some(""))
                .with_child(content.into()),
        )
        .with_optional_child(mask)
        .with_child(status_region(attempt.map(|_| operation)))
}

/// Screen readers announce a live region that changes, not one that arrives with its text already
/// in place, so this stays mounted and off screen for as long as the masked content does.
fn status_region(operation: Option<&str>) -> Container {
    Container::from_tag("span")
        .attribute("role", "status")
        .style("position", "absolute")
        .style("width", "1px")
        .style("height", "1px")
        .style("overflow", "hidden")
        .style("clip-path", "inset(50%)")
        .with_optional_child(operation.map(ToString::to_string))
}

/// Reports a failed attempt. An expired deadline is reported as no response within the time limit,
/// since the gateway timeout's own message only repeats what the countdown showed, and gets a retry
/// button if `on_retry` is set. Any other error is shown verbatim, without a retry.
///
/// Pass `None` as `on_retry` when the current input is invalid and cannot be sent again.
pub fn failure_notice(
    error: &anyhow::Error,
    endpoint: &str,
    on_retry: Option<Callback<()>>,
) -> Row {
    if !is_gateway_timeout(error) {
        return error_message(&error.to_string());
    }

    let secs = REMOTE_ONBOARDING_TIMEOUT_SECS;
    let notice = error_message(&tr!(
        "No response from {0} within {n} second."
            | "No response from {0} within {n} seconds." % secs,
        endpoint
    ));
    match on_retry {
        Some(on_retry) => notice.gap(2).with_child(retry_button(on_retry)),
        None => notice,
    }
}

/// Offers the retry on its own, for pages that already report each failure separately.
pub fn retry_row(on_retry: Callback<()>) -> Row {
    Row::new()
        .padding(2)
        .gap(2)
        .class(AlignItems::Center)
        .with_child(retry_button(on_retry))
}

pub fn is_gateway_timeout(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<proxmox_client::Error>(),
            Some(proxmox_client::Error::Api(StatusCode::GATEWAY_TIMEOUT, _))
        )
    })
}

pub fn is_gateway_timeout_client(error: &proxmox_client::Error) -> bool {
    matches!(
        error,
        proxmox_client::Error::Api(StatusCode::GATEWAY_TIMEOUT, _)
    )
}

/// Unlike in a button bar, a flat button in page content would read as plain text, so the retry
/// button is elevated.
fn retry_button(on_retry: Callback<()>) -> Button {
    Button::new(tr!("Retry"))
        .class("pwt-button-elevated")
        .on_activate(move |_| on_retry.emit(()))
}

fn elapsed_secs(started_at_millis: f64, now_millis: f64) -> u64 {
    ((now_millis - started_at_millis).max(0.0) / 1_000.0).floor() as u64
}

fn deadline_millis(started_at_millis: f64) -> f64 {
    started_at_millis + REMOTE_ONBOARDING_TIMEOUT_SECS as f64 * 1_000.0
}

#[cfg(target_arch = "wasm32")]
pub(super) fn monotonic_now_millis() -> f64 {
    web_sys::window()
        .and_then(|window| window.performance())
        .expect("browser does not provide the Performance API")
        .now()
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn monotonic_now_millis() -> f64 {
    use std::sync::OnceLock;
    use std::time::Instant;

    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_time_uses_the_fixed_start_instant() {
        assert_eq!(elapsed_secs(1_000.0, 1_999.0), 0);
        assert_eq!(elapsed_secs(1_000.0, 3_500.0), 2);
        assert_eq!(elapsed_secs(3_000.0, 2_000.0), 0);
    }

    #[test]
    fn deadline_uses_the_onboarding_timeout_and_fixed_start() {
        assert_eq!(deadline_millis(1_000.0), 181_000.0);
    }

    #[test]
    fn the_hint_only_shows_once_the_wait_is_out_of_the_ordinary() {
        assert!(slow_hint(HINT_AFTER_SECS - 1).is_none());
        assert!(slow_hint(HINT_AFTER_SECS).is_some());
    }

    #[test]
    fn the_countdown_follows_the_deadline_and_saturates() {
        assert!(countdown_text(0).contains("180 seconds"));
        assert!(countdown_text(HINT_AFTER_SECS).contains("165 seconds"));
        assert!(countdown_text(REMOTE_ONBOARDING_TIMEOUT_SECS - 1).contains("1 second)"));
        assert!(countdown_text(REMOTE_ONBOARDING_TIMEOUT_SECS + 5).contains("0 seconds"));
    }

    #[test]
    fn gateway_timeout_is_found_through_the_error_chain() {
        let error = anyhow::Error::new(proxmox_client::Error::Api(
            StatusCode::GATEWAY_TIMEOUT,
            "operation timed out".to_string(),
        ))
        .context("remote probe failed");

        assert!(is_gateway_timeout(&error));
        assert!(!is_gateway_timeout(&anyhow::format_err!(
            "operation returned HTTP 504"
        )));
        assert!(!is_gateway_timeout(&anyhow::Error::new(
            proxmox_client::Error::Api(StatusCode::BAD_GATEWAY, "failed".to_string())
        )));
    }
}
