use uuid::Uuid;

pub const DEFAULT_MEMORY_MODULE: &str = "general";

pub fn normalized_memory_module(value: &str) -> Option<String> {
    let token = normalized_token(value);
    if token.is_empty() {
        return None;
    }
    let module = match token.as_str() {
        "front" | "frontend" | "ui" | "react" | "web" => "frontend",
        "backend" | "rust" | "tauri" | "desktop" => "tauri",
        "term" | "terminal" | "pty" | "shell" => "terminal",
        "memory" | "aimemory" | "ai_memory" | "context" => "memory",
        "git" | "worktree" | "diff" => "git",
        "release" | "build" | "bundle" | "updater" | "packaging" => "release",
        "remote" | "mobile" | "handoff" | "ssh" => "remote",
        "pet" | "pets" | "companion" => "pet",
        "settings" | "config" | "preferences" => "settings",
        "performance" | "perf" | "metrics" => "performance",
        _ => token.as_str(),
    };
    Some(module.chars().take(48).collect())
}

pub fn parse_uuid_string(value: &str) -> Option<String> {
    let normalized = normalized_non_empty(value)?;
    Uuid::parse_str(&normalized).ok()?;
    Some(normalized)
}

pub fn valid_summary_content(value: &str) -> Option<String> {
    let content = normalized_non_empty(value)?;
    if content.starts_with("version=") && content.lines().count() == 1 {
        return None;
    }
    Some(content)
}

pub(super) fn normalized_non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(super) fn normalized_token(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .collect()
}
