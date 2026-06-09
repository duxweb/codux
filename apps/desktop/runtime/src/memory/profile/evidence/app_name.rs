fn infer_project_app_name(
    package: Option<&Value>,
    composer: Option<&Value>,
    tauri: Option<&Value>,
    pyproject: Option<&str>,
    go_mod: Option<&str>,
    root: &Path,
) -> Option<String> {
    tauri
        .and_then(|value| value.get("productName"))
        .and_then(Value::as_str)
        .and_then(|value| normalized_string(Some(value)))
        .or_else(|| json_string_field(package, "name"))
        .or_else(|| json_string_field(composer, "name"))
        .or_else(|| pyproject_project_name(pyproject))
        .or_else(|| go_module_name(go_mod))
        .or_else(|| {
            root.file_name()
                .and_then(|value| value.to_str())
                .and_then(|value| normalized_string(Some(value)))
        })
}

fn json_string_field(value: Option<&Value>, key: &str) -> Option<String> {
    value
        .and_then(|value| value.get(key))
        .and_then(Value::as_str)
        .and_then(|value| normalized_string(Some(value)))
}

fn pyproject_project_name(text: Option<&str>) -> Option<String> {
    let mut in_project = false;
    for line in text?.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_project = trimmed == "[project]" || trimmed == "[tool.poetry]";
            continue;
        }
        if in_project && trimmed.starts_with("name") {
            return value_after_equals(trimmed);
        }
    }
    None
}

fn go_module_name(text: Option<&str>) -> Option<String> {
    text?.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed
            .strip_prefix("module ")
            .and_then(|value| normalized_string(Some(value)))
    })
}

fn value_after_equals(line: &str) -> Option<String> {
    line.split_once('=')
        .map(|(_, value)| value.trim().trim_matches('"').trim_matches('\''))
        .and_then(|value| normalized_string(Some(value)))
}
