use serde_json::Value;

pub(super) fn llm_json_values(raw: &str) -> Vec<Value> {
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

pub(super) fn json_object_candidates(raw: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let chars = raw.char_indices().collect::<Vec<_>>();
    for (start_index, (_, ch)) in chars.iter().enumerate() {
        if *ch != '{' {
            continue;
        }
        let mut depth = 0_i32;
        let mut in_string = false;
        let mut escaped = false;
        for (offset, current) in chars.iter().skip(start_index).copied() {
            if escaped {
                escaped = false;
                continue;
            }
            if current == '\\' && in_string {
                escaped = true;
                continue;
            }
            if current == '"' {
                in_string = !in_string;
                continue;
            }
            if in_string {
                continue;
            }
            if current == '{' {
                depth += 1;
            } else if current == '}' {
                depth -= 1;
                if depth == 0 {
                    let start = chars[start_index].0;
                    let end = offset + current.len_utf8();
                    candidates.push(raw[start..end].to_string());
                    break;
                }
            }
        }
    }
    candidates
}
