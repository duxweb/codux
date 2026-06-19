use super::helpers::{normalized_memory_module, normalized_non_empty, parse_uuid_string};
use super::types::{
    MemoryExtractionItem, MemoryExtractionResponse, MemoryKind, MemoryScope, MemoryTier,
};
use serde_json::Value;
use std::collections::HashSet;

pub fn decode_extraction_response(raw: &str) -> Result<MemoryExtractionResponse, String> {
    decode_extraction_response_detailed(raw).map_err(|error| error.to_string())
}

pub fn decode_extraction_response_detailed(raw: &str) -> Result<MemoryExtractionResponse, String> {
    let stripped = strip_markdown_code_fences(raw);
    for value in llm_json_values(&stripped) {
        if let Some(response) = parse_extraction_value(&value) {
            return Ok(response);
        }
    }
    Err(malformed_json_error(raw))
}

pub fn should_stop_memory_queue_after_error(error: &str) -> bool {
    let message = error.to_lowercase();
    [
        "api key",
        "no available ai provider",
        "no enabled ai provider",
        "no provider",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

fn llm_json_values(raw: &str) -> Vec<Value> {
    let mut values = Vec::new();
    push_unique_json_value(&mut values, serde_json::from_str::<Value>(raw).ok());
    push_unique_json_value(&mut values, llm_json_repair::parse::<Value>(raw).ok());
    for candidate in json_object_candidates(raw) {
        push_unique_json_value(&mut values, serde_json::from_str::<Value>(&candidate).ok());
        push_unique_json_value(
            &mut values,
            llm_json_repair::parse::<Value>(&candidate).ok(),
        );
    }
    values
}

fn push_unique_json_value(values: &mut Vec<Value>, value: Option<Value>) {
    let Some(value) = value else {
        return;
    };
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn malformed_json_error(raw: &str) -> String {
    let preview = raw
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(240)
        .collect::<String>();
    if preview.is_empty() {
        "Memory extraction provider returned malformed memory JSON: empty response.".to_string()
    } else {
        format!("Memory extraction provider returned malformed memory JSON: {preview}")
    }
}

fn strip_markdown_code_fences(raw: &str) -> String {
    let trimmed = raw.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }
    trimmed
        .lines()
        .filter(|line| !line.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_extraction_value(value: &Value) -> Option<MemoryExtractionResponse> {
    if let Some(array) = value.as_array() {
        let working_add = array
            .iter()
            .filter_map(parse_extraction_item)
            .collect::<Vec<_>>();
        if working_add.is_empty() {
            return None;
        }
        return Some(MemoryExtractionResponse {
            working_add,
            ..MemoryExtractionResponse::default()
        });
    }
    let object = value.as_object()?;
    let nested = ["memory", "response", "result"]
        .iter()
        .filter_map(|key| object.get(*key))
        .find_map(parse_extraction_value);
    if nested.is_some() {
        return nested;
    }
    let user_summary = string_from_keys(
        value,
        &[
            "user_summary",
            "userSummary",
            "user-summary",
            "global_summary",
        ],
    );
    let mut working_add = array_from_keys(
        value,
        &[
            "working_add",
            "workingAdd",
            "working-add",
            "memories",
            "memory_entries",
            "items",
        ],
    )
    .into_iter()
    .filter_map(parse_extraction_item)
    .collect::<Vec<_>>();
    if working_add.is_empty() {
        if let Some(item) = parse_extraction_item(value) {
            working_add.push(item);
        }
    }
    let working_archive = string_array_from_keys(
        value,
        &[
            "working_archive",
            "workingArchive",
            "working-archive",
            "archive_ids",
        ],
    );
    let merged_entry_ids = string_array_from_keys(
        value,
        &[
            "merged_entry_ids",
            "mergedEntryIDs",
            "merged-entry-ids",
            "merged_ids",
        ],
    );
    let project_profile_refresh_recommended = bool_from_keys(
        value,
        &[
            "project_profile_refresh_recommended",
            "projectProfileRefreshRecommended",
            "refresh_project_profile",
            "refreshProjectProfile",
            "project_profile_stale",
            "projectProfileStale",
        ],
    )
    .unwrap_or(false);
    Some(MemoryExtractionResponse {
        user_summary,
        working_add,
        working_archive,
        merged_entry_ids,
        project_profile_refresh_recommended,
    })
}

fn parse_extraction_item(value: &Value) -> Option<MemoryExtractionItem> {
    let content = string_from_keys(value, &["content", "memory", "text", "summary", "value"])?;
    let mut merge_with = uuid_array_from_keys(
        value,
        &[
            "merge_with",
            "mergeWith",
            "merge-entry-id",
            "merge_entry_id",
        ],
    );
    merge_with = unique_strings(merge_with);
    let replace = uuid_array_from_keys(
        value,
        &[
            "replace",
            "replace_id",
            "replaceId",
            "supersedes",
            "supersedes_id",
        ],
    )
    .into_iter()
    .next();
    let mut archive = uuid_array_from_keys(value, &["archive", "archive_ids", "archiveIds"]);
    archive = unique_strings(archive);
    Some(MemoryExtractionItem {
        scope: string_from_keys(value, &["scope", "target", "level"])
            .map(|value| MemoryScope::from_str(&value)),
        module_key: string_from_keys(value, &["module_key", "moduleKey", "module", "area"])
            .and_then(|value| normalized_memory_module(&value)),
        tier: string_from_keys(value, &["tier", "priority", "stability"])
            .map(|value| MemoryTier::from_str(&value)),
        kind: string_from_keys(value, &["kind", "type", "category", "memory_type"])
            .map(|value| MemoryKind::from_str(&value))
            .unwrap_or(MemoryKind::Fact),
        content,
        rationale: string_from_keys(value, &["rationale", "reason", "context", "source", "why"]),
        merge_with,
        replace,
        archive,
        skip_reason: string_from_keys(value, &["skip_reason", "skipReason", "skip"]),
    })
}

fn json_object_candidates(raw: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let bytes = raw.as_bytes();
    for (start, byte) in bytes.iter().enumerate() {
        if *byte != b'{' && *byte != b'[' {
            continue;
        }
        let mut stack = Vec::new();
        let mut in_string = false;
        let mut escaped = false;
        for (offset, current) in bytes[start..].iter().enumerate() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if *current == b'\\' {
                    escaped = true;
                } else if *current == b'"' {
                    in_string = false;
                }
                continue;
            }
            match *current {
                b'"' => in_string = true,
                b'{' | b'[' => stack.push(*current),
                b'}' => {
                    if stack.pop() != Some(b'{') {
                        break;
                    }
                    if stack.is_empty() {
                        candidates.push(raw[start..=start + offset].to_string());
                        break;
                    }
                }
                b']' => {
                    if stack.pop() != Some(b'[') {
                        break;
                    }
                    if stack.is_empty() {
                        candidates.push(raw[start..=start + offset].to_string());
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    candidates
}

fn string_from_keys(value: &Value, keys: &[&str]) -> Option<String> {
    let object = value.as_object()?;
    for key in keys {
        if let Some(value) = object
            .get(*key)
            .and_then(|value| value.as_str())
            .and_then(normalized_non_empty)
        {
            return Some(value);
        }
    }
    None
}

fn bool_from_keys(value: &Value, keys: &[&str]) -> Option<bool> {
    let object = value.as_object()?;
    for key in keys {
        let Some(value) = object.get(*key) else {
            continue;
        };
        if let Some(value) = value.as_bool() {
            return Some(value);
        }
        if let Some(text) = value.as_str().map(|text| text.trim().to_lowercase()) {
            match text.as_str() {
                "true" | "yes" | "1" => return Some(true),
                "false" | "no" | "0" => return Some(false),
                _ => {}
            }
        }
    }
    None
}

fn array_from_keys<'a>(value: &'a Value, keys: &[&str]) -> Vec<&'a Value> {
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    for key in keys {
        if let Some(array) = object.get(*key).and_then(|value| value.as_array()) {
            return array.iter().collect();
        }
    }
    Vec::new()
}

fn string_array_from_keys(value: &Value, keys: &[&str]) -> Vec<String> {
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    for key in keys {
        let Some(value) = object.get(*key) else {
            continue;
        };
        if let Some(array) = value.as_array() {
            return array
                .iter()
                .filter_map(|item| item.as_str().and_then(normalized_non_empty))
                .collect();
        }
        if let Some(text) = value.as_str().and_then(normalized_non_empty) {
            return vec![text];
        }
    }
    Vec::new()
}

fn uuid_array_from_keys(value: &Value, keys: &[&str]) -> Vec<String> {
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    for key in keys {
        let Some(value) = object.get(*key) else {
            continue;
        };
        if let Some(array) = value.as_array() {
            return array
                .iter()
                .filter_map(|item| item.as_str().and_then(parse_uuid_string))
                .collect();
        }
        if let Some(uuid) = value.as_str().and_then(parse_uuid_string) {
            return vec![uuid];
        }
    }
    Vec::new()
}

fn unique_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}
