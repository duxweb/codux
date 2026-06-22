// Structured transcript parsing for JSONL coding tools (Claude, Codex).
//
// The on-disk transcripts are JSONL: one JSON event per line. Reading them as
// raw text (the legacy path) feeds the extractor noisy, half-truncated JSON in
// which the code-fence / tool-prefix filters never fire, because the fences and
// prefixes live inside escaped JSON strings. This module parses each event and
// keeps only the user/assistant natural-language turns -- dropping tool calls,
// tool output, reasoning/thinking, system-injected context, and fenced code
// blocks. The result is clean `User:` / `Assistant:` lines, already windowed to
// the most recent turns and token-capped, so the structured path bypasses the
// raw line compactor entirely.

/// Upper bound on a single turn after cleaning. Tool output and code are already
/// removed, so anything past this is genuine long prose; we keep the head (where
/// the intent usually lives) and bound the tail to cap per-turn token cost.
const MAX_TURN_CHARS: usize = 1500;

/// Parse a Claude/Codex JSONL transcript into clean conversational text. Returns
/// `None` only when the file is not parseable as structured JSONL for this tool
/// (the caller then falls back to the raw reader); returns `Some` -- possibly an
/// empty string -- once any line parsed, so a noise-only transcript does not
/// silently fall back to the raw (noisy) path.
pub(super) fn parse_structured_transcript(
    path: &str,
    tool: &str,
    line_limit: i32,
    token_limit: i32,
) -> Option<String> {
    if !matches!(tool, "claude" | "codex") {
        return None;
    }
    let file = fs::File::open(path).ok()?;
    let turn_limit = line_limit.max(1) as usize;
    let mut turns: std::collections::VecDeque<String> =
        std::collections::VecDeque::with_capacity(turn_limit.min(1024));
    let mut parsed_any = false;
    for raw in BufReader::new(file).lines().map_while(Result::ok) {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        parsed_any = true;
        let turn = match tool {
            "claude" => parse_claude_turn(&value),
            "codex" => parse_codex_turn(&value),
            _ => None,
        };
        let Some(turn) = turn else {
            continue;
        };
        if turns.len() == turn_limit {
            turns.pop_front();
        }
        turns.push_back(turn);
    }
    if !parsed_any {
        return None;
    }
    let text = turns.into_iter().collect::<Vec<_>>().join("\n");
    let max_chars = (token_limit.max(1) as usize).saturating_mul(4);
    let text = if text.chars().count() > max_chars {
        tail_chars(&text, max_chars)
    } else {
        text
    };
    Some(normalized_string(Some(&text)).unwrap_or_default())
}

fn parse_claude_turn(value: &serde_json::Value) -> Option<String> {
    // System-injected context and subagent (sidechain) conversations are not the
    // main session's durable intent.
    if value
        .get("isMeta")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
        || value
            .get("isSidechain")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    {
        return None;
    }
    let event_type = value.get("type").and_then(serde_json::Value::as_str)?;
    if event_type != "user" && event_type != "assistant" {
        return None;
    }
    let message = value.get("message")?;
    let role = message
        .get("role")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(event_type);
    let text = match message.get("content")? {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Array(blocks) => collect_text_blocks(blocks, &["text"]),
        _ => return None,
    };
    finalize_turn(role, &text)
}

fn parse_codex_turn(value: &serde_json::Value) -> Option<String> {
    if value.get("type").and_then(serde_json::Value::as_str)? != "response_item" {
        return None;
    }
    let payload = value.get("payload")?;
    if payload.get("type").and_then(serde_json::Value::as_str)? != "message" {
        return None;
    }
    let role = payload.get("role").and_then(serde_json::Value::as_str)?;
    // Skip the `developer` role -- it carries the system prompt / instructions,
    // not conversational content.
    if role != "user" && role != "assistant" {
        return None;
    }
    let blocks = payload.get("content")?.as_array()?;
    let text = collect_text_blocks(blocks, &["input_text", "output_text", "text"]);
    finalize_turn(role, &text)
}

fn collect_text_blocks(blocks: &[serde_json::Value], kinds: &[&str]) -> String {
    blocks
        .iter()
        .filter(|block| {
            block
                .get("type")
                .and_then(serde_json::Value::as_str)
                .map(|kind| kinds.contains(&kind))
                .unwrap_or(false)
        })
        .filter_map(|block| block.get("text").and_then(serde_json::Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
}

fn finalize_turn(role: &str, text: &str) -> Option<String> {
    let cleaned = if role == "user" {
        clean_user_freeform(text)
    } else {
        text.to_string()
    };
    let cleaned = strip_code_fences(&cleaned);
    let one_line = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.is_empty() {
        return None;
    }
    let bounded = if one_line.chars().count() > MAX_TURN_CHARS {
        format!(
            "{} [truncated]",
            one_line
                .chars()
                .take(MAX_TURN_CHARS)
                .collect::<String>()
                .trim()
        )
    } else {
        one_line
    };
    let label = if role == "user" { "User" } else { "Assistant" };
    Some(format!("{label}: {bounded}"))
}

/// Strip Codux/CLI-injected wrapper sections (system reminders, command output,
/// environment/instruction blocks) from a free-form user message. These are
/// machine noise, not the user's durable intent.
fn clean_user_freeform(text: &str) -> String {
    const WRAPPERS: &[(&str, &str)] = &[
        ("<system-reminder>", "</system-reminder>"),
        ("<local-command-stdout>", "</local-command-stdout>"),
        ("<local-command-stderr>", "</local-command-stderr>"),
        ("<local-command-caveat>", "</local-command-caveat>"),
        ("<command-name>", "</command-name>"),
        ("<command-message>", "</command-message>"),
        ("<command-args>", "</command-args>"),
        ("<environment_context>", "</environment_context>"),
        ("<user_instructions>", "</user_instructions>"),
    ];
    let mut out = text.to_string();
    for (open, close) in WRAPPERS {
        out = remove_between(&out, open, close);
    }
    out
}

fn remove_between(text: &str, open: &str, close: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(open) {
        result.push_str(&rest[..start]);
        let after = &rest[start + open.len()..];
        match after.find(close) {
            Some(end) => rest = &after[end + close.len()..],
            // No closing tag: drop everything from the opener onward.
            None => return result,
        }
    }
    result.push_str(rest);
    result
}

/// Drop fenced code blocks, keeping only the surrounding prose. The user wants
/// memory built from textual content, not pasted code.
fn strip_code_fences(text: &str) -> String {
    let mut kept = Vec::new();
    let mut in_block = false;
    let mut dropped = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_block = !in_block;
            dropped = true;
            continue;
        }
        if in_block {
            dropped = true;
            continue;
        }
        kept.push(line);
    }
    let mut joined = kept.join("\n");
    if dropped {
        joined.push_str(" [code omitted]");
    }
    joined
}

#[cfg(test)]
mod parse_tests {
    use super::{parse_claude_turn, parse_codex_turn};
    use serde_json::json;

    #[test]
    fn claude_keeps_only_text_blocks_and_strips_code_and_noise() {
        // tool_use / tool_result / thinking blocks are dropped.
        let assistant = json!({
            "type": "assistant",
            "message": {"role": "assistant", "content": [
                {"type": "thinking", "thinking": "internal reasoning"},
                {"type": "text", "text": "Use `cargo check`.\n```rust\nfn main(){}\n```\nDone."},
                {"type": "tool_use", "name": "Bash", "input": {"command": "ls"}}
            ]}
        });
        let turn = parse_claude_turn(&assistant).unwrap();
        assert!(turn.starts_with("Assistant: "));
        assert!(turn.contains("Use `cargo check`."));
        assert!(turn.contains("Done."));
        assert!(turn.contains("[code omitted]"));
        assert!(!turn.contains("fn main"));
        assert!(!turn.contains("internal reasoning"));

        // user string content keeps prose but drops injected wrappers.
        let user = json!({
            "type": "user",
            "message": {"role": "user", "content":
                "Fix the bug<system-reminder>ignore me</system-reminder> please"}
        });
        let turn = parse_claude_turn(&user).unwrap();
        assert_eq!(turn, "User: Fix the bug please");
    }

    #[test]
    fn claude_drops_meta_sidechain_and_tool_result_turns() {
        let meta = json!({"type": "user", "isMeta": true,
            "message": {"role": "user", "content": "system noise"}});
        assert!(parse_claude_turn(&meta).is_none());

        let sidechain = json!({"type": "assistant", "isSidechain": true,
            "message": {"role": "assistant", "content": [{"type": "text", "text": "sub"}]}});
        assert!(parse_claude_turn(&sidechain).is_none());

        let tool_result = json!({"type": "user",
            "message": {"role": "user", "content": [{"type": "tool_result", "content": "big output"}]}});
        assert!(parse_claude_turn(&tool_result).is_none());
    }

    #[test]
    fn codex_keeps_user_and_assistant_messages_and_skips_developer() {
        let user = json!({"type": "response_item", "payload": {"type": "message", "role": "user",
            "content": [{"type": "input_text", "text": "build the feature"}]}});
        assert_eq!(parse_codex_turn(&user).unwrap(), "User: build the feature");

        let assistant = json!({"type": "response_item", "payload": {"type": "message", "role": "assistant",
            "content": [{"type": "output_text", "text": "on it"}]}});
        assert_eq!(parse_codex_turn(&assistant).unwrap(), "Assistant: on it");

        let developer = json!({"type": "response_item", "payload": {"type": "message", "role": "developer",
            "content": [{"type": "input_text", "text": "system instructions"}]}});
        assert!(parse_codex_turn(&developer).is_none());

        let reasoning = json!({"type": "response_item", "payload": {"type": "reasoning"}});
        assert!(parse_codex_turn(&reasoning).is_none());
    }
}
