pub(super) fn normalize_scope(scope: &str) -> &'static str {
    if scope == "user" { "user" } else { "project" }
}

pub(super) fn normalized_non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn optional_sql_text(value: Option<&str>) -> rusqlite::types::Value {
    value
        .map(|value| rusqlite::types::Value::Text(value.to_string()))
        .unwrap_or(rusqlite::types::Value::Null)
}

fn max_optional_f64(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

pub(super) fn decode_string_array(value: Option<&str>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<String>>(value)
        .unwrap_or_else(|_| {
            value
                .split(',')
                .map(|item| item.trim().to_string())
                .collect()
        })
        .into_iter()
        .filter(|item| !item.is_empty())
        .collect()
}

fn truncate(value: String, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}
