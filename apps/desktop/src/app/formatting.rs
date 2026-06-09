use codux_runtime::{i18n::translate, settings::locale_from_language_setting};
use std::time::{SystemTime, UNIX_EPOCH};

pub(in crate::app) fn compact_number(value: i64) -> String {
    let abs = value.saturating_abs();
    if abs >= 1_000_000_000 {
        compact_unit(value, 1_000_000_000.0, "B")
    } else if abs >= 1_000_000 {
        compact_unit(value, 1_000_000.0, "M")
    } else if abs >= 1_000 {
        compact_unit(value, 1_000.0, "K")
    } else {
        value.to_string()
    }
}

fn compact_unit(value: i64, divisor: f64, suffix: &str) -> String {
    let scaled = value as f64 / divisor;
    let abs_scaled = scaled.abs();
    let formatted = if abs_scaled >= 100.0 {
        format!("{scaled:.0}")
    } else if abs_scaled >= 10.0 {
        format!("{scaled:.1}")
    } else {
        format!("{scaled:.2}")
    };
    format!(
        "{}{}",
        formatted.trim_end_matches('0').trim_end_matches('.'),
        suffix
    )
}

pub(in crate::app) fn relative_time_label_for_language(timestamp: f64, language: &str) -> String {
    let locale = locale_from_language_setting(language);
    let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
    if timestamp <= 0.0 {
        return tr("time.relative.just_now", "Just now");
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(timestamp);
    let seconds = (now - timestamp).max(0.0);

    if seconds < 60.0 {
        tr("time.relative.just_now", "Just now")
    } else if seconds < 3600.0 {
        tr("time.relative.minutes_ago_format", "%d minutes ago")
            .replace("%d", &((seconds / 60.0).floor() as i64).to_string())
    } else if seconds < 86_400.0 {
        tr("time.relative.hours_ago_format", "%d hours ago")
            .replace("%d", &((seconds / 3600.0).floor() as i64).to_string())
    } else if seconds < 604_800.0 {
        tr("time.relative.days_ago_format", "%d days ago")
            .replace("%d", &((seconds / 86_400.0).floor() as i64).to_string())
    } else {
        tr("time.relative.weeks_ago_format", "%d weeks ago")
            .replace("%d", &((seconds / 604_800.0).floor() as i64).to_string())
    }
}
