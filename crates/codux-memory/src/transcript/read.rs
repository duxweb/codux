pub(super) fn read_transcript_file(
    path: &str,
    line_limit: i32,
    token_limit: i32,
) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let line_limit = line_limit.max(1) as usize;
    let mut lines = std::collections::VecDeque::with_capacity(line_limit);
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if lines.len() == line_limit {
            lines.pop_front();
        }
        lines.push_back(line);
    }
    let mut text = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            text.push('\n');
        }
        text.push_str(line);
    }
    let max_chars = (token_limit.max(1) as usize).saturating_mul(4);
    if text.chars().count() > max_chars {
        text = tail_chars(&text, max_chars);
    }
    normalized_string(Some(&text))
}

pub(super) fn prepare_transcript_for_memory(text: &str, settings: &MemorySettings) -> String {
    let line_limit = settings.max_extraction_transcript_lines.max(1) as usize;
    let token_limit = settings.max_extraction_transcript_tokens.max(1);
    let tail = text
        .lines()
        .rev()
        .filter_map(|line| normalized_string(Some(line.trim())))
        .take(line_limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    compact_transcript_for_memory(&tail, token_limit)
        .unwrap_or_else(|| trim_memory_text(&tail, token_limit))
}

pub(super) fn resolve_transcript_for_task_with_settings(
    task: &MemoryExtractionTask,
    project: &MemoryProjectContext,
    settings: &MemorySettings,
) -> Result<String, String> {
    let line_limit = settings.max_extraction_transcript_lines;
    let token_limit = settings.max_extraction_transcript_tokens;
    // Structured path for JSONL tools: parse into clean conversational turns,
    // dropping tool calls/results, reasoning, and code blocks. It is already
    // windowed + token-capped, so it bypasses the raw line compactor.
    if let Some(text) = structured_transcript_for_task(task, project, line_limit, token_limit) {
        return Ok(text);
    }
    // Fallback (opencode or unparseable JSONL): raw read + line compactor.
    resolve_transcript_for_task_raw(task, project, line_limit, token_limit)
        .map(|text| prepare_transcript_for_memory(&text, settings))
}

/// Try to build a clean transcript from a JSONL tool by parsing the session file
/// directly. Returns `None` for non-JSONL tools or when no candidate file parses,
/// so the caller falls back to the raw reader.
fn structured_transcript_for_task(
    task: &MemoryExtractionTask,
    project: &MemoryProjectContext,
    line_limit: i32,
    token_limit: i32,
) -> Option<String> {
    let tool = task.tool.to_lowercase();
    if !matches!(tool.as_str(), "claude" | "codex") {
        return None;
    }
    let workspace_path = task
        .workspace_path
        .as_deref()
        .and_then(|value| normalized_string(Some(value)))
        .unwrap_or_else(|| project.workspace_path.clone());
    let mut candidates: Vec<String> = Vec::new();
    if Path::new(&task.transcript_path).is_file() {
        candidates.push(task.transcript_path.clone());
    }
    match tool.as_str() {
        "claude" => candidates.extend(
            claude_project_log_paths(&workspace_path)
                .into_iter()
                .map(|path| path.display().to_string()),
        ),
        "codex" => {
            if let Some(path) = find_codex_rollout_path(&workspace_path, &task.session_id) {
                candidates.push(path.display().to_string());
            }
        }
        _ => {}
    }
    candidates
        .into_iter()
        .find_map(|path| parse_structured_transcript(&path, &tool, line_limit, token_limit))
}

pub(super) fn resolve_transcript_for_task(
    task: &MemoryExtractionTask,
    project: &MemoryProjectContext,
) -> Result<String, String> {
    resolve_transcript_for_task_raw(task, project, 80, 8000)
}

fn resolve_transcript_for_task_raw(
    task: &MemoryExtractionTask,
    project: &MemoryProjectContext,
    line_limit: i32,
    token_limit: i32,
) -> Result<String, String> {
    let workspace_path = task
        .workspace_path
        .as_deref()
        .and_then(|value| normalized_string(Some(value)))
        .unwrap_or_else(|| project.workspace_path.clone());
    let tool = task.tool.to_lowercase();
    if Path::new(&task.transcript_path).is_file() {
        if tool == "opencode" && task.transcript_path.ends_with(".db") {
            if let Some(text) = fetch_opencode_transcript(
                &workspace_path,
                &task.session_id,
                &task.transcript_path,
                line_limit,
                token_limit,
            )
            {
                return Ok(text);
            }
        } else if let Some(text) = read_transcript_file(&task.transcript_path, line_limit, token_limit)
        {
            return Ok(text);
        }
    }
    match tool.as_str() {
        "claude" => {
            for path in claude_project_log_paths(&workspace_path) {
                if let Some(text) =
                    read_transcript_file(&path.display().to_string(), line_limit, token_limit)
                {
                    return Ok(text);
                }
            }
        }
        "codex" => {
            if let Some(path) = find_codex_rollout_path(&workspace_path, &task.session_id)
                && let Some(text) =
                    read_transcript_file(&path.display().to_string(), line_limit, token_limit)
            {
                return Ok(text);
            }
        }
        "opencode" => {
            if let Some(text) = fetch_opencode_transcript(
                &workspace_path,
                &task.session_id,
                &opencode_database_path().display().to_string(),
                line_limit,
                token_limit,
            ) {
                return Ok(text);
            }
        }
        _ => {}
    }
    Err("Unable to resolve transcript for memory extraction.".to_string())
}
