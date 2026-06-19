pub(super) fn resolve_transcript_source(
    session: &MemorySessionSnapshot,
    project: &MemoryProjectContext,
) -> Option<TranscriptSource> {
    let tool = normalized_string(Some(&session.tool))?.to_lowercase();
    let session_id = session_identifier(session);

    if let Some(source) = session
        .transcript_path
        .as_deref()
        .and_then(|path| normalized_string(Some(path)))
        .and_then(|path| transcript_source_if_readable(&path, &tool, &session_id, false))
    {
        return Some(source);
    }

    match tool.as_str() {
        "claude" => {
            let ai_session = normalized_string(session.ai_session_id.as_deref())?;
            claude_project_log_paths(&project.workspace_path)
                .into_iter()
                .find(|path| {
                    claude_log_contains_session(path, &ai_session, &project.workspace_path)
                })
                .and_then(|path| {
                    transcript_source_if_readable(
                        &path.display().to_string(),
                        &tool,
                        &ai_session,
                        false,
                    )
                })
        }
        "codex" => {
            let ai_session = normalized_string(session.ai_session_id.as_deref())?;
            find_codex_rollout_path(&project.workspace_path, &ai_session).and_then(|path| {
                transcript_source_if_readable(
                    &path.display().to_string(),
                    &tool,
                    &ai_session,
                    false,
                )
            })
        }
        "gemini" => {
            let files = gemini_session_paths(&project.workspace_path);
            let matching = session
                .ai_session_id
                .as_deref()
                .and_then(|ai_session| normalized_string(Some(ai_session)))
                .and_then(|ai_session| {
                    files
                        .iter()
                        .find(|path| {
                            path.file_name()
                                .and_then(|value| value.to_str())
                                .map(|name| name.contains(&ai_session))
                                .unwrap_or(false)
                        })
                        .cloned()
                })
                .or_else(|| files.first().cloned());
            matching.and_then(|path| {
                transcript_source_if_readable(
                    &path.display().to_string(),
                    &tool,
                    &session_id,
                    false,
                )
            })
        }
        "opencode" => transcript_source_if_readable(
            &opencode_database_path().display().to_string(),
            &tool,
            &session_id,
            true,
        ),
        _ => None,
    }
}

fn transcript_source_if_readable(
    path: &str,
    tool: &str,
    session_id: &str,
    allow_database: bool,
) -> Option<TranscriptSource> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() == 0 {
        return None;
    }
    if !allow_database && read_transcript_file(path, 80, 8000).is_none() {
        return None;
    }
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_secs_f64())
        .unwrap_or(0.0);
    Some(TranscriptSource {
        location: path.to_string(),
        fingerprint: sha256_hex(&format!(
            "{tool}|{session_id}|{path}|{}|{modified_at}",
            metadata.len()
        )),
    })
}

fn claude_log_contains_session(path: &Path, session_id: &str, project_path: &str) -> bool {
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .take(40)
        .any(|line| {
            line.contains(session_id)
                && (line.contains(project_path)
                    || serde_json::from_str::<serde_json::Value>(&line)
                        .ok()
                        .and_then(|value| {
                            value
                                .get("cwd")
                                .and_then(|value| value.as_str())
                                .map(|cwd| paths_equivalent(Some(cwd), project_path))
                        })
                        .unwrap_or(false))
        })
}
