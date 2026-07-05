use std::sync::OnceLock;
use std::time::Duration;

use gpui::{Animation, AnimationExt as _, Transformation, ease_in_out, percentage};

use super::ai_runtime_status::AgentLifecycleState;
use super::*;

pub(in crate::app) fn agent_lifecycle_color(state: AgentLifecycleState) -> gpui::Hsla {
    match state {
        AgentLifecycleState::Working => color(theme::ACCENT),
        AgentLifecycleState::Waiting => color(theme::ORANGE),
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
    use cocoa::base::{id, YES};
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let workspace: id = msg_send![class!(NSWorkspace), sharedWorkspace];
        let enabled: cocoa::base::BOOL = msg_send![workspace, accessibilityDisplayShouldReduceMotion];
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

pub(in crate::app) fn agent_lifecycle_status_dot(
    lifecycle_state: AgentLifecycleState,
    animation_id: &str,
) -> AnyElement {
    match lifecycle_state {
        AgentLifecycleState::Idle => div().into_any_element(),
        AgentLifecycleState::Working => {
            if reduce_motion_enabled() {
                return div()
                    .flex_none()
                    .size(px(6.0))
                    .rounded_full()
                    .bg(color(theme::ACCENT))
                    .into_any_element();
            }

            Icon::new(HeroIconName::ArrowPath)
                .size(px(8.0))
                .text_color(color(theme::ACCENT))
                .with_animation(
                    SharedString::from(animation_id.to_string()),
                    Animation::new(Duration::from_millis(900))
                        .repeat()
                        .with_easing(ease_in_out),
                    |icon, delta| icon.transform(Transformation::rotate(percentage(delta))),
                )
                .into_any_element()
        }
        AgentLifecycleState::Waiting => div()
            .flex_none()
            .size(px(6.0))
            .rounded_full()
            .bg(color(theme::ORANGE))
            .into_any_element(),
        AgentLifecycleState::Completed => Icon::new(HeroIconName::Check)
            .size(px(10.0))
            .text_color(color(theme::GREEN))
            .into_any_element(),
    }
}
