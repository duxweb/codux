use serde_json::{Map, Value};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ai_runtime::state::normalized_string;

pub(super) fn parse_iso8601_seconds(value: &str) -> Option<f64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| {
            date.timestamp() as f64 + f64::from(date.timestamp_subsec_micros()) / 1_000_000.0
        })
}

pub(super) fn now_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

pub(super) fn json_i64(value: Option<&Value>) -> i64 {
    value.and_then(|value| value.as_i64()).unwrap_or(0)
}

pub(super) fn first_string_deep(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Object(object) => {
            for key in keys {
                if let Some(value) = object
                    .get(*key)
                    .and_then(|value| value.as_str())
                    .and_then(|value| normalized_string(Some(value)))
                {
                    return Some(value);
                }
            }
            for child in object.values() {
                if let Some(value) = first_string_deep(child, keys) {
                    return Some(value);
                }
            }
            None
        }
        Value::Array(items) => items.iter().find_map(|item| first_string_deep(item, keys)),
        _ => None,
    }
}

pub(super) fn first_object_deep<'a>(
    value: &'a Value,
    keys: &[&str],
) -> Option<&'a Map<String, Value>> {
    match value {
        Value::Object(object) => {
            for key in keys {
                if let Some(child) = object.get(*key).and_then(|value| value.as_object()) {
                    return Some(child);
                }
            }
            for child in object.values() {
                if let Some(value) = first_object_deep(child, keys) {
                    return Some(value);
                }
            }
            None
        }
        Value::Array(items) => items.iter().find_map(|item| first_object_deep(item, keys)),
        _ => None,
    }
}
