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

pub(super) fn resolve_transcript_for_task(
    task: &MemoryExtractionTask,
    project: &MemoryProjectContext,
) -> Result<String, String> {
    let workspace_path = task
        .workspace_path
        .as_deref()
        .and_then(|value| normalized_string(Some(value)))
        .unwrap_or_else(|| project.workspace_path.clone());
    let tool = task.tool.to_lowercase();
    if Path::new(&task.transcript_path).is_file() {
        if tool == "opencode" && task.transcript_path.ends_with(".db") {
            if let Some(text) =
                fetch_opencode_transcript(&workspace_path, &task.session_id, &task.transcript_path)
            {
                return Ok(text);
            }
        } else if let Some(text) = read_transcript_file(&task.transcript_path, 80, 8000) {
            return Ok(text);
        }
    }
    match tool.as_str() {
        "claude" => {
            for path in claude_project_log_paths(&workspace_path) {
                if let Some(text) = read_transcript_file(&path.display().to_string(), 80, 8000) {
                    return Ok(text);
                }
            }
        }
        "codex" => {
            if let Some(path) = find_codex_rollout_path(&workspace_path, &task.session_id) {
                if let Some(text) = read_transcript_file(&path.display().to_string(), 80, 8000) {
                    return Ok(text);
                }
            }
        }
        "gemini" => {
            for path in gemini_session_paths(&workspace_path) {
                if let Some(text) = read_transcript_file(&path.display().to_string(), 80, 8000) {
                    return Ok(text);
                }
            }
        }
        "opencode" => {
            if let Some(text) = fetch_opencode_transcript(
                &workspace_path,
                &task.session_id,
                &opencode_database_path().display().to_string(),
            ) {
                return Ok(text);
            }
        }
        _ => {}
    }
    Err("Unable to resolve transcript for memory extraction.".to_string())
}
