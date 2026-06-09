use super::{AIHistoryService, AISessionDetail, AISessionForkRequest, AISessionForkResult};
use crate::runtime_paths::runtime_temp_dir;
use serde_json::Value;
use std::{
    collections::{HashSet, VecDeque},
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const MAX_TAIL_BYTES: u64 = 16 * 1024 * 1024;
const MAX_LINES_PER_FILE: usize = 100_000;
const MAX_TRANSCRIPT_FILES: usize = 32;
const MAX_SNIPPETS: usize = 240;
const MAX_SNIPPET_CHARS: usize = 2400;
const MAX_PROMPT_CHARS: usize = 120_000;
const MAX_JSON_DEPTH: usize = 10;
const LARGE_STRING_LIMIT: usize = 20_000;
const LARGE_BASE64_LIMIT: usize = 2048;

impl AIHistoryService {
    pub fn fork_project_session(
        &self,
        request: AISessionForkRequest,
    ) -> Result<AISessionForkResult, String> {
        let detail = self.project_session_detail(&request.project_path, &request.session_id)?;
        let mut builder = ForkPromptBuilder::new(request, detail);
        builder.collect_snippets();
        builder.write_prompt()
    }
}

struct ForkPromptBuilder {
    request: AISessionForkRequest,
    detail: AISessionDetail,
    snippets: Vec<String>,
    omitted_items: usize,
}

impl ForkPromptBuilder {
    fn new(request: AISessionForkRequest, detail: AISessionDetail) -> Self {
        Self {
            request,
            detail,
            snippets: Vec::new(),
            omitted_items: 0,
        }
    }

    fn collect_snippets(&mut self) {
        let files = self.detail.files.clone();
        let mut seen_paths = HashSet::new();
        let mut paths = Vec::new();
        for file in files.iter() {
            let path = PathBuf::from(&file.file_path);
            if !path.is_file() || !seen_paths.insert(path.clone()) {
                continue;
            }
            paths.push(path);
            if paths.len() >= MAX_TRANSCRIPT_FILES {
                break;
            }
        }

        for path in paths.into_iter().rev() {
            let mut snippets = if path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
            {
                self.collect_json_file_snippets(&path)
            } else {
                self.collect_line_file_snippets(&path)
            };
            if snippets.is_empty() {
                continue;
            }
            self.snippets.append(&mut snippets);
            self.snippets = last_snippets(std::mem::take(&mut self.snippets), MAX_SNIPPETS);
        }
    }

    fn collect_line_file_snippets(&mut self, path: &Path) -> Vec<String> {
        let mut snippets = Vec::new();
        for line in read_recent_lines(path, MAX_TAIL_BYTES, MAX_LINES_PER_FILE) {
            if let Some(snippet) = cleaned_snippet_from_line(&line, &mut self.omitted_items) {
                snippets.push(snippet);
            }
        }
        last_snippets(snippets, MAX_SNIPPETS)
    }

    fn collect_json_file_snippets(&mut self, path: &Path) -> Vec<String> {
        let Ok(mut file) = File::open(path) else {
            return Vec::new();
        };
        let Ok(metadata) = file.metadata() else {
            return Vec::new();
        };
        if metadata.len() > MAX_TAIL_BYTES {
            self.omitted_items += 1;
            return Vec::new();
        }
        let mut data = String::new();
        if file.read_to_string(&mut data).is_err() {
            return Vec::new();
        }
        let Ok(value) = serde_json::from_str::<Value>(&data) else {
            return Vec::new();
        };
        last_snippets(
            cleaned_json_document_snippets(&value, &mut self.omitted_items),
            MAX_SNIPPETS,
        )
    }

    fn write_prompt(&mut self) -> Result<AISessionForkResult, String> {
        let prompt = self.prompt_text();
        let dir = runtime_temp_dir()
            .join("session-forks")
            .join(safe_path_component(&self.request.project_id));
        fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        let path = dir.join(format!(
            "{}-{}.md",
            safe_path_component(&self.request.session_id),
            Uuid::new_v4().simple()
        ));
        fs::write(&path, prompt.as_bytes()).map_err(|error| error.to_string())?;
        Ok(AISessionForkResult {
            title: format!(
                "{}: {}",
                self.request.target_tool.display_name(),
                short_title(&self.detail.title)
            ),
            prompt_path: path.display().to_string(),
            prompt_chars: prompt.chars().count(),
            omitted_items: self.omitted_items,
        })
    }

    fn prompt_text(&mut self) -> String {
        let mut text = String::new();
        push_limited(
            &mut text,
            &format!(
                "# Continue Cleaned AI Session\n\n\
                 You are continuing an AI coding session in Codux with {}.\n\
                 Codux injects its project memory, user memory, SSH command context, and tool permissions separately at launch. Do not ask the user to paste Codux memory, and do not duplicate memory content in this prompt.\n\n",
                self.request.target_tool.display_name()
            ),
        );
        push_limited(&mut text, "## Project\n");
        push_limited(
            &mut text,
            &format!("- Name: {}\n", self.request.project_name),
        );
        push_limited(
            &mut text,
            &format!("- Path: {}\n\n", self.request.project_path),
        );
        push_limited(&mut text, "## Original Session\n");
        push_limited(&mut text, &format!("- Title: {}\n", self.detail.title));
        push_limited(&mut text, &format!("- Source: {}\n", self.detail.source));
        if let Some(model) = self
            .detail
            .files
            .iter()
            .find_map(|file| (!file.model.trim().is_empty()).then(|| file.model.clone()))
        {
            push_limited(&mut text, &format!("- Last model: {model}\n"));
        }
        push_limited(
            &mut text,
            &format!(
                "- Requests: {}\n- Tokens: {}\n- Cached input tokens: {}\n\n",
                self.detail.request_count,
                self.detail.total_tokens,
                self.detail.cached_input_tokens
            ),
        );
        push_limited(&mut text, "## Handoff Instructions\n");
        push_limited(
            &mut text,
            "- Continue from the cleaned context below.\n\
             - Treat omitted inline images, base64 payloads, screenshots, and oversized tool outputs as unavailable unless the user explicitly provides them again.\n\
             - Prefer inspecting repository files directly over relying on stale transcript text.\n\
             - Keep using the current project path as the workspace.\n\n",
        );
        push_limited(&mut text, "## Recent Cleaned Transcript\n");
        if self.snippets.is_empty() {
            push_limited(
                &mut text,
                "No readable recent transcript text was available. Continue using the project files and Codux-injected memory.\n",
            );
        } else {
            let snippets = self.snippets.clone();
            for (index, snippet) in snippets.iter().enumerate() {
                if text.chars().count() >= MAX_PROMPT_CHARS {
                    self.omitted_items += 1;
                    break;
                }
                push_limited(&mut text, &format!("\n### Excerpt {}\n", index + 1));
                push_limited(&mut text, snippet);
                push_limited(&mut text, "\n");
            }
        }
        push_limited(
            &mut text,
            &format!(
                "\n## Cleanup Notes\n- Omitted unsafe or oversized transcript items: {}\n",
                self.omitted_items
            ),
        );
        text
    }
}

fn read_recent_lines(path: &Path, max_bytes: u64, max_lines: usize) -> Vec<String> {
    let Ok(mut file) = File::open(path) else {
        return Vec::new();
    };
    let Ok(metadata) = file.metadata() else {
        return Vec::new();
    };
    let len = metadata.len();
    let start = len.saturating_sub(max_bytes);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut data = String::new();
    if file.read_to_string(&mut data).is_err() {
        return Vec::new();
    }
    if start > 0
        && let Some(index) = data.find('\n')
    {
        data = data[index + 1..].to_string();
    }
    let mut lines = VecDeque::new();
    for line in data.lines().filter(|line| !line.trim().is_empty()) {
        if lines.len() == max_lines {
            lines.pop_front();
        }
        lines.push_back(line.to_string());
    }
    lines.into_iter().collect()
}

fn cleaned_snippet_from_line(line: &str, omitted_items: &mut usize) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return cleaned_snippet_from_json(&value, omitted_items);
    }
    clamp_snippet(sanitize_text(trimmed, omitted_items))
}

fn cleaned_snippet_from_json(value: &Value, omitted_items: &mut usize) -> Option<String> {
    if is_claude_row(value) {
        return cleaned_claude_row_snippet(value, omitted_items);
    }
    let row_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !row_type.is_empty()
        && !matches!(row_type, "response_item" | "message" | "user" | "assistant")
    {
        *omitted_items += 1;
        return None;
    }
    if matches!(
        row_type,
        "session_meta" | "turn_context" | "event_msg" | "token_count"
    ) {
        return None;
    }

    let payload = value.get("payload").unwrap_or(value);
    let payload_type = payload
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or(row_type);
    if matches!(
        payload_type,
        "reasoning" | "session_meta" | "turn_context" | "token_count" | "task_started"
    ) {
        *omitted_items += 1;
        return None;
    }

    match payload_type {
        "message" => cleaned_message_snippet(payload, omitted_items),
        "function_call" => cleaned_function_call_snippet(payload, omitted_items),
        "function_call_output" => cleaned_function_output_snippet(payload, omitted_items),
        _ => cleaned_generic_message_snippet(payload, omitted_items),
    }
}

fn cleaned_json_document_snippets(value: &Value, omitted_items: &mut usize) -> Vec<String> {
    let mut snippets = Vec::new();
    if let Some(messages) = value.get("messages").and_then(Value::as_array) {
        for message in messages
            .iter()
            .rev()
            .take(MAX_SNIPPETS)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            if let Some(snippet) = cleaned_message_object_snippet(message, omitted_items) {
                snippets.push(snippet);
            }
        }
        return snippets;
    }
    if let Some(snippet) = cleaned_message_object_snippet(value, omitted_items) {
        snippets.push(snippet);
    }
    snippets
}

fn cleaned_claude_row_snippet(value: &Value, omitted_items: &mut usize) -> Option<String> {
    let row_type = value.get("type").and_then(Value::as_str)?;
    if !matches!(row_type, "user" | "assistant") {
        *omitted_items += 1;
        return None;
    }
    let message = value.get("message").unwrap_or(value);
    cleaned_message_object_snippet(message, omitted_items)
        .or_else(|| cleaned_message_snippet(value, omitted_items))
}

fn is_claude_row(value: &Value) -> bool {
    value
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|row_type| matches!(row_type, "user" | "assistant"))
        && (value.get("sessionId").is_some() || value.get("message").is_some())
}

fn cleaned_message_object_snippet(value: &Value, omitted_items: &mut usize) -> Option<String> {
    let role = value
        .get("role")
        .or_else(|| value.pointer("/message/role"))
        .and_then(Value::as_str)
        .or_else(|| value.get("type").and_then(Value::as_str))
        .unwrap_or_default();
    if !matches!(role, "user" | "assistant") {
        *omitted_items += 1;
        return None;
    }
    let content = value
        .get("content")
        .or_else(|| value.pointer("/message/content"))
        .or_else(|| value.get("text"))?;
    let mut texts = Vec::new();
    collect_message_text(content, 0, &mut texts, omitted_items);
    let body = clean_joined_texts(texts, omitted_items)?;
    clamp_snippet(format!("{role}:\n{body}"))
}

fn cleaned_message_snippet(payload: &Value, omitted_items: &mut usize) -> Option<String> {
    let role = payload
        .get("role")
        .or_else(|| payload.pointer("/message/role"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !matches!(role, "user" | "assistant") {
        *omitted_items += 1;
        return None;
    }

    let content = payload
        .get("content")
        .or_else(|| payload.pointer("/message/content"))
        .unwrap_or(payload);
    let mut texts = Vec::new();
    collect_message_text(content, 0, &mut texts, omitted_items);
    let body = clean_joined_texts(texts, omitted_items)?;
    clamp_snippet(format!("{role}:\n{body}"))
}

fn cleaned_function_call_snippet(payload: &Value, omitted_items: &mut usize) -> Option<String> {
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("tool");
    let arguments = payload
        .get("arguments")
        .and_then(Value::as_str)
        .map(|text| sanitize_text(text, omitted_items))
        .filter(|text| useful_text(text))
        .unwrap_or_default();
    if arguments.is_empty() {
        return clamp_snippet(format!("tool call: {name}"));
    }
    clamp_snippet(format!("tool call: {name}\n{arguments}"))
}

fn cleaned_function_output_snippet(payload: &Value, omitted_items: &mut usize) -> Option<String> {
    let output = payload
        .get("output")
        .and_then(Value::as_str)
        .map(|text| sanitize_text(text, omitted_items))?;
    if !useful_text(&output) {
        return None;
    }
    clamp_snippet(format!("tool output:\n{output}"))
}

fn cleaned_generic_message_snippet(payload: &Value, omitted_items: &mut usize) -> Option<String> {
    let role = payload
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !role.is_empty() && !matches!(role, "user" | "assistant") {
        *omitted_items += 1;
        return None;
    }
    let mut texts = Vec::new();
    collect_message_text(payload, 0, &mut texts, omitted_items);
    clean_joined_texts(texts, omitted_items).and_then(clamp_snippet)
}

fn clean_joined_texts(texts: Vec<String>, omitted_items: &mut usize) -> Option<String> {
    let joined = texts
        .into_iter()
        .map(|text| sanitize_text(&text, omitted_items))
        .filter(|text| useful_text(text))
        .take(8)
        .collect::<Vec<_>>()
        .join("\n");
    (!joined.trim().is_empty()).then_some(joined)
}

fn collect_message_text(
    value: &Value,
    depth: usize,
    texts: &mut Vec<String>,
    omitted_items: &mut usize,
) {
    if depth > MAX_JSON_DEPTH || texts.len() >= 16 {
        return;
    }
    match value {
        Value::String(text) => {
            if useful_text(text) {
                texts.push(text.clone());
            }
        }
        Value::Array(items) => {
            if items.len() > 32 {
                *omitted_items += 1;
            }
            for item in items.iter().take(32) {
                collect_message_text(item, depth + 1, texts, omitted_items);
            }
        }
        Value::Object(map) => {
            if map
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|value| matches!(value, "thinking" | "reasoning" | "attachment"))
            {
                *omitted_items += 1;
                return;
            }
            for (key, child) in map {
                if is_unsafe_payload_key(key) {
                    *omitted_items += 1;
                    continue;
                }
                if key == "role"
                    || key == "type"
                    || key == "call_id"
                    || key == "thinking"
                    || key == "signature"
                {
                    continue;
                }
                if is_text_key(key) {
                    if let Value::String(text) = child {
                        if useful_text(text) {
                            texts.push(text.clone());
                        }
                        continue;
                    }
                }
                collect_message_text(child, depth + 1, texts, omitted_items);
            }
        }
        _ => {}
    }
}

fn sanitize_text(text: &str, omitted_items: &mut usize) -> String {
    let text = strip_codux_injected_context(text, omitted_items);
    if is_runtime_context_text(&text) {
        *omitted_items += 1;
        return String::new();
    }
    if text.len() > LARGE_STRING_LIMIT && is_base64_like(&text) {
        *omitted_items += 1;
        return "[omitted large base64 payload]".to_string();
    }
    let mut output = String::new();
    for token in text.split_whitespace() {
        if token.starts_with("data:image") {
            *omitted_items += 1;
            output.push_str("[omitted inline image]");
        } else if token.len() > LARGE_BASE64_LIMIT && is_base64_like(token) {
            *omitted_items += 1;
            output.push_str("[omitted large base64 payload]");
        } else {
            output.push_str(token);
        }
        output.push(' ');
    }
    output.trim().to_string()
}

fn is_runtime_context_text(text: &str) -> bool {
    let normalized = text.trim_start();
    normalized.starts_with("<environment_context>")
        || normalized.starts_with("<permissions instructions>")
        || normalized.starts_with("<turn_meta>")
        || normalized.starts_with("<turn_aborted>")
        || normalized.starts_with("<collaboration_mode>")
        || normalized.starts_with("<skills_instructions>")
        || normalized.starts_with("<plugins_instructions>")
        || text.contains("# Continue Cleaned AI Session")
        || text.contains("<environment_context>")
        || text.contains("<permissions instructions>")
        || text.contains("<turn_meta>")
        || text.contains("<turn_aborted>")
        || text.contains("<collaboration_mode>")
        || text.contains("<skills_instructions>")
        || text.contains("<plugins_instructions>")
}

fn strip_codux_injected_context(text: &str, omitted_items: &mut usize) -> String {
    let markers = [
        "# Codux Memory",
        "## Global Prompt",
        "## Project Profile",
        "Project profile present:",
        "Active entries:",
        "Core entries:",
        "Working entries:",
        "Archived entries:",
    ];
    if !markers.iter().any(|marker| text.contains(marker)) {
        return text.to_string();
    }
    *omitted_items += 1;
    let mut kept = Vec::new();
    let mut skipping = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "# Codux Memory"
            || trimmed == "## Global Prompt"
            || trimmed == "## Project Profile"
            || trimmed == "## Recent Memory"
            || trimmed == "## Extra Context"
        {
            skipping = true;
            continue;
        }
        if skipping && trimmed.starts_with("# ") && trimmed != "# Codux Memory" {
            skipping = false;
        }
        if !skipping {
            kept.push(line);
        }
    }
    let cleaned = kept.join("\n").trim().to_string();
    if cleaned.is_empty() {
        "[omitted Codux injected memory context]".to_string()
    } else {
        format!("[omitted Codux injected memory context]\n{cleaned}")
    }
}

fn useful_text(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.starts_with("[omitted") {
        return true;
    }
    trimmed.len() >= 2
        && !trimmed.starts_with('{')
        && !trimmed.starts_with('[')
        && !trimmed.eq_ignore_ascii_case("null")
}

fn clamp_snippet(text: String) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut snippet = trimmed.chars().take(MAX_SNIPPET_CHARS).collect::<String>();
    if trimmed.chars().count() > MAX_SNIPPET_CHARS {
        snippet.push_str("\n[omitted remaining oversized excerpt]");
    }
    Some(snippet)
}

fn is_text_key(key: &str) -> bool {
    matches!(
        key,
        "text"
            | "content"
            | "message"
            | "output"
            | "input"
            | "summary"
            | "title"
            | "command"
            | "stdout"
            | "stderr"
            | "output_text"
            | "input_text"
    )
}

fn is_unsafe_payload_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase();
    normalized.contains("image")
        || normalized.contains("screenshot")
        || normalized.contains("base64")
        || normalized == "dataurl"
        || normalized == "data_url"
        || normalized == "blob"
}

fn is_base64_like(text: &str) -> bool {
    let mut chars = 0usize;
    let mut base64_chars = 0usize;
    for ch in text.chars() {
        if ch.is_whitespace() {
            continue;
        }
        chars += 1;
        if ch.is_ascii_alphanumeric() || matches!(ch, '+' | '/' | '=' | '-' | '_') {
            base64_chars += 1;
        }
    }
    chars > 0 && (base64_chars as f32 / chars as f32) > 0.95
}

fn push_limited(output: &mut String, text: &str) {
    let remaining = MAX_PROMPT_CHARS.saturating_sub(output.chars().count());
    if remaining == 0 {
        return;
    }
    output.push_str(&text.chars().take(remaining).collect::<String>());
}

fn short_title(title: &str) -> String {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return "New session".to_string();
    }
    trimmed.chars().take(48).collect()
}

fn safe_path_component(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    sanitized.trim_matches('-').chars().take(80).collect()
}

fn last_snippets(snippets: Vec<String>, max: usize) -> Vec<String> {
    let count = snippets.len();
    snippets
        .into_iter()
        .skip(count.saturating_sub(max))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_history::AISessionForkTarget;
    use rusqlite::params;

    #[test]
    fn cleaned_snippet_omits_inline_images_and_large_base64() {
        let payload = "A".repeat(4096);
        let line = serde_json::json!({
            "type": "response_item",
            "payload": {
                "message": {
                    "role": "user",
                    "content": [
                        {"type": "input_text", "text": "Please continue this task"},
                        {"type": "input_image", "image_url": format!("data:image/png;base64,{payload}")},
                        {"type": "output_text", "text": payload}
                    ]
                }
            }
        })
        .to_string();

        let mut omitted = 0;
        let snippet = cleaned_snippet_from_line(&line, &mut omitted).expect("snippet");

        assert!(snippet.contains("Please continue this task"));
        assert!(!snippet.contains("data:image"));
        assert!(omitted >= 1);

        let mut omitted_text = 0;
        let snippet = cleaned_snippet_from_line(
            &serde_json::json!({"text": "A".repeat(LARGE_STRING_LIMIT + 1)}).to_string(),
            &mut omitted_text,
        )
        .expect("large snippet");
        assert!(snippet.contains("[omitted large base64 payload]"));
        assert_eq!(omitted_text, 1);
    }

    #[test]
    fn fork_prompt_does_not_duplicate_codux_memory() {
        let support_dir = temp_support_dir("fork");
        let transcript_dir = support_dir.join("transcripts");
        fs::create_dir_all(&transcript_dir).unwrap();
        let transcript = transcript_dir.join("session.jsonl");
        fs::write(
            &transcript,
            serde_json::json!({
                "message": {
                    "role": "user",
                    "content": "# Codux Memory\n## Global Prompt\nDo not duplicate this injected prompt.\n# User Request\nImplement the fix."
                }
            })
            .to_string(),
        )
        .unwrap();
        create_fork_history_db(&support_dir, &transcript);

        let result = AIHistoryService::new(support_dir.clone())
            .fork_project_session(AISessionForkRequest {
                project_id: "project-1".to_string(),
                project_name: "Project One".to_string(),
                project_path: "/tmp/project-one".to_string(),
                session_id: "session-1".to_string(),
                target_tool: AISessionForkTarget::Codex,
            })
            .expect("fork");
        let prompt = fs::read_to_string(result.prompt_path).unwrap();

        assert!(prompt.contains("Codux injects its project memory"));
        assert!(!prompt.contains("## Global Prompt"));
        assert!(!prompt.contains("\n# Codux Memory\n"));
        assert!(prompt.contains("[omitted Codux injected memory context]"));
        assert!(prompt.len() <= MAX_PROMPT_CHARS + 1024);
        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn fork_prompt_skips_runtime_noise_and_reasoning() {
        let support_dir = temp_support_dir("fork-noise");
        let transcript_dir = support_dir.join("transcripts");
        fs::create_dir_all(&transcript_dir).unwrap();
        let transcript = transcript_dir.join("session.jsonl");
        let lines = [
            serde_json::json!({
                "timestamp": "2026-06-06T05:07:21.167Z",
                "type": "session_meta",
                "payload": {"id": "session-1", "base_instructions": {"text": "developer rules must not appear"}}
            })
            .to_string(),
            serde_json::json!({
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "developer",
                    "content": [{"type": "input_text", "text": "<permissions instructions> sandbox rules"}]
                }
            })
            .to_string(),
            serde_json::json!({
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "<environment_context> noisy env"}]
                }
            })
            .to_string(),
            serde_json::json!({
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "# Continue Cleaned AI Session\n<permissions instructions> old handoff"}]
                }
            })
            .to_string(),
            serde_json::json!({
                "type": "response_item",
                "payload": {"type": "reasoning", "text": "gAAAAABsecret"}
            })
            .to_string(),
            serde_json::json!({
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "帮我看下现在能连接哪些ssh"}]
                }
            })
            .to_string(),
            serde_json::json!({
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "我先看本机 SSH 配置。"}]
                }
            })
            .to_string(),
        ];
        fs::write(&transcript, lines.join("\n")).unwrap();
        create_fork_history_db(&support_dir, &transcript);

        let result = AIHistoryService::new(support_dir.clone())
            .fork_project_session(AISessionForkRequest {
                project_id: "project-1".to_string(),
                project_name: "Project One".to_string(),
                project_path: "/tmp/project-one".to_string(),
                session_id: "session-1".to_string(),
                target_tool: AISessionForkTarget::Claude,
            })
            .expect("fork");
        let prompt = fs::read_to_string(result.prompt_path).unwrap();

        assert!(prompt.contains("帮我看下现在能连接哪些ssh"));
        assert!(prompt.contains("我先看本机 SSH 配置。"));
        assert!(!prompt.contains("developer rules must not appear"));
        assert!(!prompt.contains("<permissions instructions>"));
        assert!(!prompt.contains("<environment_context>"));
        assert!(!prompt.contains("old handoff"));
        assert!(!prompt.contains("gAAAAABsecret"));
        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn fork_prompt_reads_later_files_when_recent_files_are_noise() {
        let support_dir = temp_support_dir("fork-later-files");
        let transcript_dir = support_dir.join("transcripts");
        fs::create_dir_all(&transcript_dir).unwrap();

        let mut transcripts = Vec::new();
        for index in 0..8 {
            let transcript = transcript_dir.join(format!("session-{index}.jsonl"));
            if index < 6 {
                fs::write(
                    &transcript,
                    [
                        serde_json::json!({"type": "session_meta", "payload": {"text": "noise"}})
                            .to_string(),
                        serde_json::json!({
                            "type": "response_item",
                            "payload": {"type": "reasoning", "text": "hidden reasoning"}
                        })
                        .to_string(),
                    ]
                    .join("\n"),
                )
                .unwrap();
            } else {
                fs::write(
                    &transcript,
                    [
                        serde_json::json!({
                            "type": "response_item",
                            "payload": {
                                "type": "message",
                                "role": "user",
                                "content": [{"type": "input_text", "text": format!("useful user turn {index}")}]
                            }
                        })
                        .to_string(),
                        serde_json::json!({
                            "type": "response_item",
                            "payload": {
                                "type": "message",
                                "role": "assistant",
                                "content": [{"type": "output_text", "text": format!("useful assistant turn {index}")}]
                            }
                        })
                        .to_string(),
                    ]
                    .join("\n"),
                )
                .unwrap();
            }
            transcripts.push((transcript, 100.0 - index as f64));
        }
        create_fork_history_db_with_files(&support_dir, &transcripts);

        let result = AIHistoryService::new(support_dir.clone())
            .fork_project_session(AISessionForkRequest {
                project_id: "project-1".to_string(),
                project_name: "Project One".to_string(),
                project_path: "/tmp/project-one".to_string(),
                session_id: "session-1".to_string(),
                target_tool: AISessionForkTarget::Codex,
            })
            .expect("fork");
        let prompt = fs::read_to_string(result.prompt_path).unwrap();

        assert!(prompt.contains("useful user turn 6"));
        assert!(prompt.contains("useful assistant turn 7"));
        assert!(!prompt.contains("hidden reasoning"));
        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn fork_prompt_keeps_enough_recent_transcript_turns() {
        let support_dir = temp_support_dir("fork-rich");
        let transcript_dir = support_dir.join("transcripts");
        fs::create_dir_all(&transcript_dir).unwrap();
        let transcript = transcript_dir.join("session.jsonl");
        let lines = (0..300)
            .map(|index| {
                serde_json::json!({
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "role": if index % 2 == 0 { "user" } else { "assistant" },
                        "content": [{"type": "input_text", "text": format!("rich transcript turn {index:03}")}]
                    }
                })
                .to_string()
            })
            .collect::<Vec<_>>();
        fs::write(&transcript, lines.join("\n")).unwrap();
        create_fork_history_db(&support_dir, &transcript);

        let result = AIHistoryService::new(support_dir.clone())
            .fork_project_session(AISessionForkRequest {
                project_id: "project-1".to_string(),
                project_name: "Project One".to_string(),
                project_path: "/tmp/project-one".to_string(),
                session_id: "session-1".to_string(),
                target_tool: AISessionForkTarget::Codex,
            })
            .expect("fork");
        let prompt = fs::read_to_string(result.prompt_path).unwrap();
        let excerpt_count = prompt.matches("### Excerpt ").count();

        assert_eq!(excerpt_count, MAX_SNIPPETS);
        assert!(!prompt.contains("rich transcript turn 00"));
        assert!(prompt.contains("rich transcript turn 060"));
        assert!(prompt.contains("rich transcript turn 299"));
        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    #[ignore]
    fn audit_local_existing_session_forks() {
        let home = std::env::var("HOME").expect("HOME");
        let support_dirs = [
            PathBuf::from(&home).join("Library/Application Support/Codux Dev"),
            PathBuf::from(&home).join("Library/Application Support/Codux"),
        ];
        let cases = [
            (
                "codex",
                "/Volumes/Web/test",
                "019e9b53-c895-7240-8b0f-8c916a145cc0",
                AISessionForkTarget::Claude,
            ),
            (
                "claude",
                "/Volumes/Web/test",
                "87b1cf6d-e932-4a85-83ff-3a786aba3e6c",
                AISessionForkTarget::Codex,
            ),
            (
                "codewhale",
                "/Volumes/Web/test",
                "b32a836b-083f-46b3-afc1-ee45b5eb510c",
                AISessionForkTarget::Codex,
            ),
        ];

        for support_dir in support_dirs {
            if !support_dir.join("ai-usage.sqlite3").is_file() {
                continue;
            }
            for (source, project_path, session_id, target_tool) in cases {
                let result = AIHistoryService::new(support_dir.clone()).fork_project_session(
                    AISessionForkRequest {
                        project_id: "local-audit".to_string(),
                        project_name: format!("local audit {source}"),
                        project_path: project_path.to_string(),
                        session_id: session_id.to_string(),
                        target_tool,
                    },
                );
                let Ok(result) = result else {
                    eprintln!(
                        "skip {source} {session_id} in {}: {}",
                        support_dir.display(),
                        result.unwrap_err()
                    );
                    continue;
                };
                let prompt = fs::read_to_string(&result.prompt_path).expect("prompt");
                println!(
                    "AUDIT\t{}\t{}\t{}\t{}\tchars={}\tomitted={}",
                    support_dir.display(),
                    source,
                    session_id,
                    result.prompt_path,
                    result.prompt_chars,
                    result.omitted_items
                );
                assert_clean_prompt(&prompt);
            }
        }
    }

    fn assert_clean_prompt(prompt: &str) {
        for forbidden in [
            "\n# Codux Memory\n",
            "## Global Prompt",
            "<permissions instructions>",
            "<environment_context>",
            "session_meta",
            "turn_context",
            "event_msg",
            "response_item\nreasoning",
            "data:image",
        ] {
            assert!(
                !prompt.contains(forbidden),
                "prompt contains forbidden marker: {forbidden}"
            );
        }
    }

    fn temp_support_dir(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("codux-ai-session-fork-{name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn create_fork_history_db(support_dir: &Path, transcript: &Path) {
        create_fork_history_db_with_files(support_dir, &[(transcript.to_path_buf(), 2.0)]);
    }

    fn create_fork_history_db_with_files(support_dir: &Path, transcripts: &[(PathBuf, f64)]) {
        let db_path = support_dir.join("ai-usage.sqlite3");
        let conn = rusqlite::Connection::open(db_path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE ai_history_file_session_link (
                source TEXT NOT NULL,
                file_path TEXT NOT NULL,
                project_path TEXT NOT NULL,
                session_key TEXT NOT NULL,
                external_session_id TEXT,
                session_title TEXT,
                last_model TEXT,
                first_seen_at REAL,
                last_seen_at REAL,
                active_duration_seconds INTEGER DEFAULT 0
            );
            CREATE TABLE ai_history_file_usage_bucket (
                project_path TEXT NOT NULL,
                source TEXT NOT NULL,
                file_path TEXT NOT NULL,
                session_key TEXT NOT NULL,
                bucket_start REAL NOT NULL,
                total_tokens INTEGER DEFAULT 0,
                cached_input_tokens INTEGER DEFAULT 0,
                request_count INTEGER DEFAULT 0
            );
            "#,
        )
        .unwrap();
        for (index, (transcript, last_seen_at)) in transcripts.iter().enumerate() {
            conn.execute(
                r#"
                INSERT INTO ai_history_file_session_link
                (source, file_path, project_path, session_key, external_session_id, session_title, last_model, first_seen_at, last_seen_at, active_duration_seconds)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                "#,
                params![
                    "codex",
                    transcript.display().to_string(),
                    "/tmp/project-one",
                    "session-1",
                    "external-1",
                    "Original title",
                    "gpt-5",
                    1.0 + index as f64,
                    last_seen_at,
                    1
                ],
            )
            .unwrap();
            conn.execute(
                r#"
                INSERT INTO ai_history_file_usage_bucket
                (project_path, source, file_path, session_key, bucket_start, total_tokens, cached_input_tokens, request_count)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    "/tmp/project-one",
                    "codex",
                    transcript.display().to_string(),
                    "session-1",
                    1.0 + index as f64,
                    100,
                    10,
                    1
                ],
            )
            .unwrap();
        }
    }
}
