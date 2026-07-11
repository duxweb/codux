use super::super::formatting::{compact_token_unit, usage_amount_label};
use super::*;
use crate::app::ui_helpers::{centered_empty_state, codux_tooltip_container, with_codux_tooltip};
use chrono::{Datelike as _, TimeZone as _, Timelike as _};
use codux_runtime::{i18n::translate, settings::locale_from_language_setting};
use gpui::Hsla;
use gpui_component::{
    Size,
    input::{Input, InputState},
    progress::Progress,
};
fn ai_sidebar_text(language: &str, key: &str, fallback: &str) -> String {
    let locale = locale_from_language_setting(language);
    translate(&locale, key, fallback)
}

mod memory_labels;
mod memory_rows;
mod memory_window;
mod stats;

pub(in crate::app) use memory_window::{MemoryManagerWindowInput, memory_manager_window_workspace};
pub(in crate::app) use stats::ai_stats_sidebar;
