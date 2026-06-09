use super::{payload::AIHookEventMetadata, tool_driver};

pub fn canonical_tool_name(tool: &str) -> Option<String> {
    tool_driver::canonical_tool_name(tool).map(str::to_string)
}

pub fn runtime_state_for_hook_kind(
    kind: &str,
    metadata: Option<&AIHookEventMetadata>,
) -> &'static str {
    match kind {
        "promptSubmitted" | "memoryRefreshing" => "responding",
        "sessionStarted" => "idle",
        "needsInput" => "needsInput",
        "turnCompleted" | "sessionEnded" => "idle",
        _ if metadata
            .and_then(|metadata| metadata.notification_type.as_deref())
            .and_then(|value| normalized_string(Some(value)))
            .is_some() =>
        {
            "needsInput"
        }
        _ => "idle",
    }
}

pub fn status_for_runtime_state(state: &str) -> &'static str {
    match state {
        "responding" => "running",
        "needsInput" => "needs-input",
        _ => "idle",
    }
}

pub fn normalized_string(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_tool_names_like_tauri_runtime() {
        assert_eq!(
            canonical_tool_name("claude-code").as_deref(),
            Some("claude")
        );
        assert_eq!(canonical_tool_name("agy").as_deref(), Some("agy"));
        assert_eq!(
            canonical_tool_name("codewhale-tui").as_deref(),
            Some("codewhale")
        );
        assert_eq!(
            canonical_tool_name("deepseek-tui").as_deref(),
            Some("codewhale")
        );
        assert_eq!(canonical_tool_name("codex").as_deref(), Some("codex"));
    }

    #[test]
    fn maps_hook_kind_to_runtime_status() {
        assert_eq!(
            status_for_runtime_state(runtime_state_for_hook_kind("promptSubmitted", None)),
            "running"
        );
        assert_eq!(
            status_for_runtime_state(runtime_state_for_hook_kind("needsInput", None)),
            "needs-input"
        );
        assert_eq!(
            status_for_runtime_state(runtime_state_for_hook_kind("turnCompleted", None)),
            "idle"
        );
    }
}
