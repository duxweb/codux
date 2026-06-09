use serde_json::{Value, value::RawValue};

pub(super) fn sanitized_preview_from_raw_values(values: &[Option<&RawValue>]) -> Option<String> {
    let parsed = values
        .iter()
        .map(|value| value.and_then(|value| serde_json::from_str::<Value>(value.get()).ok()))
        .collect::<Vec<_>>();
    let refs = parsed
        .iter()
        .map(|value| value.as_ref())
        .collect::<Vec<_>>();
    sanitized_preview_from_values(&refs)
}

pub(super) fn sanitized_preview_from_values(values: &[Option<&Value>]) -> Option<String> {
    for value in values.iter().flatten() {
        for text in flatten_text(value) {
            if let Some(preview) = sanitized_preview(&text) {
                return Some(preview);
            }
        }
    }
    None
}

pub(super) fn joined_preview_from_values(values: &[Option<&Value>]) -> Option<String> {
    let mut lines = Vec::new();
    'outer: for value in values.iter().flatten() {
        for text in flatten_text(value) {
            for line in text
                .replace("\r\n", "\n")
                .replace('\r', "\n")
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
            {
                lines.push(line.to_string());
                if lines.len() >= 3 {
                    break 'outer;
                }
            }
        }
    }
    let preview = lines.join("\n");
    sanitized_preview(&preview)
}

fn flatten_text(value: &Value) -> Vec<String> {
    match value {
        Value::String(text) => vec![text.clone()],
        Value::Array(items) => items.iter().flat_map(flatten_text).collect(),
        Value::Object(object) => ["text", "content", "message", "summary"]
            .into_iter()
            .filter_map(|key| object.get(key))
            .flat_map(flatten_text)
            .collect(),
        _ => Vec::new(),
    }
}

fn sanitized_preview(value: &str) -> Option<String> {
    let preview = value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(3)
        .collect::<Vec<_>>()
        .join("\n");
    let preview = preview.trim();
    if preview.is_empty() {
        None
    } else {
        Some(preview.chars().take(180).collect())
    }
}
