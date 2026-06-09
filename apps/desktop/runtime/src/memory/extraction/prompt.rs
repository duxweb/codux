use super::helpers::{DEFAULT_MEMORY_MODULE, normalized_non_empty};
use super::types::{PromptMemoryEntry, PromptMemorySummary};
use crate::settings::AIMemorySettings;

pub fn extraction_system_prompt() -> &'static str {
    "You extract and compact durable software-engineering memory from AI coding sessions.\n\nReturn JSON only.\nDo not include markdown fences.\nDo not include <think> blocks, reasoning text, analysis, explanations, or prose.\nThe first non-whitespace character of the response must be \"{\".\nDo not call tools, request scans, browse files, or infer facts outside the provided transcript and existing memory.\nTreat this as a deterministic memory compaction job, not a chat response."
}

pub fn make_extraction_prompt(
    transcript: &str,
    user_summary: Option<&PromptMemorySummary>,
    user_memories: &[PromptMemoryEntry],
    project_memories: &[PromptMemoryEntry],
    project_name: &str,
    output_locale: &str,
    settings: &AIMemorySettings,
) -> String {
    let output_language = memory_extraction_language_label(output_locale);
    format!(
        "Memory extraction schema: codux-memory-v4\nProject: {project_name}\n\nExisting user summary:\n{}\n\nRelevant user memories:\n{}\n\nRelevant project memories:\n{}\n\nTranscript:\n<transcript>\n{}\n</transcript>\n\nReturn minified JSON only, with no markdown and no line breaks:\n{{\"user_summary\":\"\",\"working_add\":[],\"working_archive\":[],\"merged_entry_ids\":[],\"project_profile_refresh_recommended\":false}}\n\nRules:\n- If nothing durable should be stored, return the exact empty JSON shape above.\n- Add at most 3 working_add items total. Prefer the highest value durable memories only.\n- Each working_add item must include content, kind, tier, scope, and module_key. scope must be exactly user or project; module_key must be a non-empty concise module name.\n- Optional item fields: merge_with, replace, archive, skip_reason.\n- project_profile_refresh_recommended must be true only when the transcript reveals likely project-wide changes to purpose, architecture, tech stack, major modules, or common commands. Do not set it for ordinary bug fixes, logs, or task progress.\n- merge_with and replace must be a single existing UUID string, not an array. If multiple existing memories are duplicates, set merge_with to the best target id and put the other duplicate ids in archive.\n- Use merge_with for semantic duplicates, replace for conflicts where the new memory supersedes an old entry, archive for stale or duplicate entry ids, skip_reason for candidates that should not be stored.\n- Write user_summary, working_add.content, rationale, and skip_reason in {output_language}. Preserve code identifiers, file paths, commands, URLs, API names, branch names, model/tool names, and quoted error text exactly. JSON keys and enum values must remain in English.\n- Keep each working_add.content concise: target 120-220 Chinese characters or 60-110 English words. Summarize the memory naturally; do not hard-truncate, cut mid-sentence, or drop critical qualifiers just to hit a number.\n- Keep rationale short: target one brief sentence. Summarize rather than truncating.\n- user_summary <= about {} tokens; empty string means keep existing user summary unchanged. If it would exceed the budget, rewrite it as a compact summary instead of truncating it.\n- Do not produce project_summary. Project profile is generated from repository files, not chat transcripts.\n- Extract only durable engineering memory. Omit temporary tasks, logs, timestamps, greetings, tool output, generic knowledge, and assistant-invented preferences.\n- scope=user only for explicit cross-project user habits/preferences; user entries should use module_key=\"user\".\n- Repository facts, commands, release flow, UI decisions, bugs, diagnostics, paths, APIs, and conventions must be scope=project and assigned a concise module_key such as frontend, tauri, terminal, memory, git, release, remote, pet, performance, or general.\n- Ambiguous or low-value information must be omitted.\n- kind must be preference, convention, decision, fact, or bug_lesson. tier must be core or working.",
        render_existing_summary(user_summary),
        render_existing_memories(user_memories),
        render_existing_memories(project_memories),
        trim_memory_text(transcript, settings.max_extraction_transcript_tokens),
        settings.summary_target_token_budget
    )
}

pub fn memory_extraction_language_label(locale: &str) -> &'static str {
    let normalized = locale.replace('_', "-").to_lowercase();
    if normalized.starts_with("zh-hant") {
        "Traditional Chinese"
    } else if normalized.starts_with("zh") {
        "Simplified Chinese"
    } else if normalized.starts_with("ja") {
        "Japanese"
    } else if normalized.starts_with("ko") {
        "Korean"
    } else if normalized.starts_with("fr") {
        "French"
    } else if normalized.starts_with("de") {
        "German"
    } else if normalized.starts_with("es") {
        "Spanish"
    } else if normalized.starts_with("pt") {
        "Portuguese"
    } else if normalized.starts_with("ru") {
        "Russian"
    } else {
        "English"
    }
}

pub fn trim_memory_text(text: &str, max_tokens: i32) -> String {
    let max_chars = (max_tokens.max(50) as usize * 3).max(200);
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    format!(
        "{}\n[Memory extraction input truncated]",
        text.chars()
            .take(max_chars)
            .collect::<String>()
            .trim()
            .to_string()
    )
}

fn render_existing_summary(summary: Option<&PromptMemorySummary>) -> String {
    summary
        .and_then(|summary| {
            normalized_non_empty(&summary.content)
                .map(|content| format!("version={}\n{}", summary.version, content))
        })
        .unwrap_or_else(|| "(none)".to_string())
}

fn render_existing_memories(entries: &[PromptMemoryEntry]) -> String {
    if entries.is_empty() {
        return "(none)".to_string();
    }
    entries
        .iter()
        .map(|entry| {
            if let Some(rationale) = normalized_non_empty(entry.rationale.as_deref().unwrap_or(""))
            {
                format!(
                    "- id={} module={} [{}] {} (context: {})",
                    entry.id,
                    entry.module_key.as_deref().unwrap_or(DEFAULT_MEMORY_MODULE),
                    entry.kind,
                    entry.content,
                    rationale
                )
            } else {
                format!(
                    "- id={} module={} [{}] {}",
                    entry.id,
                    entry.module_key.as_deref().unwrap_or(DEFAULT_MEMORY_MODULE),
                    entry.kind,
                    entry.content
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
