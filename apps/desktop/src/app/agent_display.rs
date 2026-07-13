use std::sync::OnceLock;
use std::time::Instant;

use gpui::{Transformation, percentage};

use super::ai_runtime_status::AgentLifecycleState;
use super::*;

pub(in crate::app) fn agent_lifecycle_color(state: AgentLifecycleState) -> gpui::Hsla {
    match state {
        AgentLifecycleState::Working => color(theme::ACCENT),
        AgentLifecycleState::Waiting => color(theme::ORANGE),
        AgentLifecycleState::Error => color(theme::RED),
        AgentLifecycleState::Warning => color(theme::ORANGE),
        AgentLifecycleState::Completed => color(theme::GREEN),
        AgentLifecycleState::Idle => color(theme::TEXT_DIM),
    }
}

pub(in crate::app) fn reduce_motion_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(detect_reduce_motion)
}

fn detect_reduce_motion() -> bool {
    #[cfg(target_os = "macos")]
    {
        if let Some(enabled) = macos_reduce_motion_enabled() {
            return enabled;
        }
        defaults_reduce_motion_enabled()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[cfg(target_os = "macos")]
fn macos_reduce_motion_enabled() -> Option<bool> {
    use cocoa::base::{YES, id};
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let workspace: id = msg_send![class!(NSWorkspace), sharedWorkspace];
        let enabled: cocoa::base::BOOL =
            msg_send![workspace, accessibilityDisplayShouldReduceMotion];
        Some(enabled == YES)
    }
}

#[cfg(target_os = "macos")]
fn defaults_reduce_motion_enabled() -> bool {
    std::process::Command::new("defaults")
        .args(["read", "com.apple.universalaccess", "reduceMotion"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim() == "1")
        .unwrap_or(false)
}

// Phase (0..1 over ~1.1s). One shared pulse task repaints the three status
// views while at least one terminal is working.
fn ping_phase() -> f32 {
    const PERIOD_SECS: f32 = 1.1;
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    (EPOCH.get_or_init(Instant::now).elapsed().as_secs_f32() / PERIOD_SECS) % 1.0
}

/// Reusable "ping" dot: a solid dot with an expanding, fading ring behind it
/// (Tailwind animate-ping style). Reduce-motion renders just the solid dot.
pub(in crate::app) fn ping_dot(dot_color: gpui::Hsla, size: f32) -> AnyElement {
    let dot = || {
        div()
            .flex_none()
            .size(px(size))
            .rounded_full()
            .bg(dot_color)
    };
    if reduce_motion_enabled() {
        return dot().into_any_element();
    }
    let phase = ping_phase();
    let ring = size * (1.0 + 1.15 * phase);
    let ring_alpha = 0.42 * (1.0 - phase);
    let offset = (size - ring) / 2.0;
    div()
        .relative()
        .flex_none()
        .size(px(size))
        .child(
            div()
                .absolute()
                .top(px(offset))
                .left(px(offset))
                .size(px(ring))
                .rounded_full()
                .bg(dot_color.opacity(ring_alpha)),
        )
        .child(dot())
        .into_any_element()
}

/// Reusable ring spinner (Tailwind animate-spin look): gpui-component's
/// loader-circle arc rotated by the current phase, advanced by the runtime
/// pulse timer (no 60fps with_animation; the SVG rasterizes once, rotation is
/// a GPU transform).
pub(in crate::app) fn spin_icon(icon_color: gpui::Hsla, size: f32) -> AnyElement {
    let icon = Icon::new(gpui_component::IconName::LoaderCircle)
        .size(px(size))
        .text_color(icon_color);
    if reduce_motion_enabled() {
        return icon.into_any_element();
    }
    icon.transform(Transformation::rotate(percentage(ping_phase())))
        .into_any_element()
}

pub(in crate::app) fn agent_lifecycle_status_dot(
    lifecycle_state: AgentLifecycleState,
) -> AnyElement {
    let inner = match lifecycle_state {
        AgentLifecycleState::Idle => return div().into_any_element(),
        AgentLifecycleState::Working => spin_icon(color(theme::ACCENT), 12.0),
        AgentLifecycleState::Waiting => div()
            .size(px(6.0))
            .rounded_full()
            .bg(color(theme::ORANGE))
            .into_any_element(),
        AgentLifecycleState::Completed => div()
            .size(px(7.0))
            .rounded_full()
            .bg(color(theme::GREEN))
            .into_any_element(),
        AgentLifecycleState::Error => div()
            .size(px(7.0))
            .rounded_full()
            .bg(color(theme::RED))
            .into_any_element(),
        AgentLifecycleState::Warning => div()
            .size(px(7.0))
            .rounded_full()
            .bg(color(theme::ORANGE))
            .into_any_element(),
    };
    // Fixed-width slot so the subtitle doesn't shift between the 12px spinner
    // and the 6px waiting dot.
    div()
        .flex_none()
        .size(px(12.0))
        .flex()
        .items_center()
        .justify_center()
        .child(inner)
        .into_any_element()
}
