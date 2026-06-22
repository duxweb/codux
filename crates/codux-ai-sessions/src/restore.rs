//! Builds the shell command that resumes a session in its originating CLI tool.
//! Single source of truth shared by the desktop "open" action and the remote
//! `ai.session` `restore` op so the two never drift on tool-specific flags.

use crate::AISessionSummary;

/// Resume command for `session`, e.g. `claude --resume <id>` / `codex resume <id>`.
/// Mirrors the per-tool flags the desktop has always used.
pub fn session_restore_command(session: &AISessionSummary) -> String {
    let tool = session.source.to_lowercase();
    let id = session
        .external_session_id
        .as_deref()
        .filter(|id| !id.trim().is_empty())
        .unwrap_or(&session.session_key);
    let quoted_id = shell_quote(id);
    if tool.contains("codex") {
        format!("codex resume {quoted_id}")
    } else if tool.contains("claude") {
        format!("claude --resume {quoted_id}")
    } else if tool.contains("agy") || tool.contains("antigravity") {
        format!("agy resume {quoted_id}")
    } else if tool.contains("gemini") {
        format!("gemini resume {quoted_id}")
    } else if tool.contains("opencode") {
        format!("opencode run --session {quoted_id}")
    } else if tool.contains("mimo") {
        format!("mimo run --session {quoted_id}")
    } else if tool.contains("codewhale") || tool.contains("deepseek") {
        format!("codewhale resume {quoted_id}")
    } else if tool.contains("kimi") {
        "kimi".to_string()
    } else {
        format!("codex resume {quoted_id}")
    }
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | ':' | '='))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}
