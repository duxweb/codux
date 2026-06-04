pub fn index_project_history_fresh_with_progress<F>(
    project: AIHistoryProjectRequest,
    mut on_progress: F,
) -> AIHistorySnapshot
where
    F: FnMut(f64, &'static str),
{
    load_project_history_with_home(project, &home_dir(), &mut on_progress)
}

pub fn load_indexed_project_history(
    project: AIHistoryProjectRequest,
) -> Result<Option<AIHistorySnapshot>> {
    let store = AIUsageStore::default();
    let conn = store.connect()?;
    store.indexed_project_snapshot(&conn, project)
}

pub fn rename_indexed_history_session(
    project: AIHistoryProjectRequest,
    session_id: String,
    title: String,
) -> Result<Option<AIHistorySnapshot>> {
    let store = AIUsageStore::default();
    let conn = store.connect()?;
    if !store.rename_project_session(&conn, &project.path, &session_id, &title)? {
        return Ok(None);
    }
    store.indexed_project_snapshot(&conn, project)
}

pub fn remove_indexed_history_session(
    project: AIHistoryProjectRequest,
    session_id: String,
) -> Result<Option<AIHistorySnapshot>> {
    let store = AIUsageStore::default();
    let conn = store.connect()?;
    if !store.remove_project_session(&conn, &project.path, &session_id)? {
        return Ok(None);
    }
    store.indexed_project_snapshot(&conn, project)
}

pub fn index_global_history_fresh(
    projects: Vec<AIHistoryProjectRequest>,
) -> AIGlobalHistorySnapshot {
    let home = home_dir();
    let mut total_tokens = 0;
    let mut cached_input_tokens = 0;
    let mut today_total_tokens = 0;
    let mut today_cached_input_tokens = 0;
    let mut project_count = 0;

    for project in projects {
        if project.path.trim().is_empty() {
            continue;
        }
        let snapshot = load_project_history_with_home(project, &home, &mut |_, _| {});
        total_tokens += snapshot.project_summary.project_total_tokens;
        cached_input_tokens += snapshot.project_summary.project_cached_input_tokens;
        today_total_tokens += snapshot.project_summary.today_total_tokens;
        today_cached_input_tokens += snapshot.project_summary.today_cached_input_tokens;
        project_count += 1;
    }

    AIGlobalHistorySnapshot {
        total_tokens,
        cached_input_tokens,
        today_total_tokens,
        today_cached_input_tokens,
        sessions: Vec::new(),
        project_count,
        indexed_at: now_seconds(),
    }
}

pub fn load_indexed_global_history(
    projects: Vec<AIHistoryProjectRequest>,
) -> Result<Option<AIGlobalHistorySnapshot>> {
    let store = AIUsageStore::default();
    let conn = store.connect()?;
    let now = now_seconds();
    let mut total_tokens = 0;
    let mut cached_input_tokens = 0;
    let mut today_total_tokens = 0;
    let mut today_cached_input_tokens = 0;
    let mut indexed_count = 0;
    let requested_count = projects
        .iter()
        .filter(|project| !project.path.trim().is_empty())
        .count();

    for project in projects {
        if project.path.trim().is_empty() {
            continue;
        }
        let Some(snapshot) = store.indexed_project_snapshot(&conn, project)? else {
            continue;
        };
        total_tokens += snapshot.project_summary.project_total_tokens;
        cached_input_tokens += snapshot.project_summary.project_cached_input_tokens;
        today_total_tokens += snapshot.project_summary.today_total_tokens;
        today_cached_input_tokens += snapshot.project_summary.today_cached_input_tokens;
        indexed_count += 1;
    }

    if requested_count > 0 && indexed_count == 0 {
        return Ok(None);
    }
    Ok(Some(AIGlobalHistorySnapshot {
        total_tokens,
        cached_input_tokens,
        today_total_tokens,
        today_cached_input_tokens,
        sessions: Vec::new(),
        project_count: indexed_count,
        indexed_at: now,
    }))
}

pub fn global_today_normalized_tokens() -> Result<i64> {
    let store = AIUsageStore::default();
    let conn = store.connect()?;
    store.global_today_normalized_tokens(&conn)
}

pub fn global_today_normalized_tokens_at(database_path: PathBuf) -> Result<i64> {
    let store = AIUsageStore::at_path(database_path);
    let conn = store.connect()?;
    store.global_today_normalized_tokens(&conn)
}

pub fn global_all_time_normalized_tokens() -> Result<i64> {
    let store = AIUsageStore::default();
    let conn = store.connect()?;
    store.global_all_time_normalized_tokens(&conn)
}

pub fn indexed_sessions_since(cutoff: Option<f64>) -> Result<Vec<AISessionSummary>> {
    let store = AIUsageStore::default();
    let conn = store.connect()?;
    store.indexed_sessions_since(&conn, cutoff)
}

pub fn normalized_project_totals_since(
    cutoff: Option<f64>,
) -> Result<Vec<crate::ai_usage_store::AIUsageProjectTotal>> {
    let store = AIUsageStore::default();
    let conn = store.connect()?;
    store.normalized_project_totals_since(&conn, cutoff)
}

fn load_project_history_with_home(
    project: AIHistoryProjectRequest,
    home: &Path,
    on_progress: &mut impl FnMut(f64, &'static str),
) -> AIHistorySnapshot {
    if project.path.trim().is_empty() {
        return build_snapshot(project, ParsedHistory::default());
    }

    on_progress(0.12, "readingSources");
    if let Ok(snapshot) = load_project_history_with_store(
        project.clone(),
        home,
        &AIUsageStore::default(),
        on_progress,
    ) {
        return snapshot;
    }

    load_project_history_without_store(project, home, on_progress)
}

fn load_project_history_without_store(
    project: AIHistoryProjectRequest,
    home: &Path,
    on_progress: &mut impl FnMut(f64, &'static str),
) -> AIHistorySnapshot {
    let mut parsed = ParsedHistory::default();
    parsed.merge(parse_claude_history(&project, home));
    on_progress(0.38, "readingSources");
    parsed.merge(parse_codex_history(&project, home));
    on_progress(0.58, "readingSources");
    parsed.merge(parse_gemini_history(&project, home));
    on_progress(0.74, "readingSources");
    parsed.merge(parse_kiro_history(&project, home));
    on_progress(0.82, "readingSources");
    parsed.merge(parse_opencode_history(&project, home));
    on_progress(0.88, "readingSources");
    on_progress(0.96, "aggregating");
    build_snapshot(project, parsed)
}

fn load_project_history_with_store(
    project: AIHistoryProjectRequest,
    home: &Path,
    store: &AIUsageStore,
    on_progress: &mut impl FnMut(f64, &'static str),
) -> Result<AIHistorySnapshot> {
    if project.path.trim().is_empty() {
        return Ok(build_snapshot(project, ParsedHistory::default()));
    }

    let conn = store.connect()?;
    for file_path in claude_project_log_paths(&project.path, home) {
        let _ = store.load_or_index_jsonl_file(
            &conn,
            "claude",
            &file_path,
            &project,
            |checkpoint| {
                let seed = checkpoint.and_then(|checkpoint| {
                    decode_checkpoint_payload(checkpoint.payload_json.as_deref())
                });
                parse_claude_history_file_snapshot(
                    &project,
                    &file_path,
                    checkpoint.map(|item| item.last_offset).unwrap_or(0),
                    seed.as_ref(),
                )
            },
            || parse_claude_history_file_snapshot(&project, &file_path, 0, None),
        )?;
    }
    on_progress(0.38, "readingSources");
    for file_path in codex_session_paths(&project.path, home) {
        let _ = store.load_or_index_jsonl_file(
            &conn,
            "codex",
            &file_path,
            &project,
            |checkpoint| {
                let seed = checkpoint.and_then(|checkpoint| {
                    decode_checkpoint_payload(checkpoint.payload_json.as_deref())
                });
                parse_codex_history_file_snapshot(
                    &project,
                    &file_path,
                    checkpoint.map(|item| item.last_offset).unwrap_or(0),
                    seed.as_ref(),
                )
            },
            || parse_codex_history_file_snapshot(&project, &file_path, 0, None),
        )?;
    }
    on_progress(0.58, "readingSources");
    for file_path in gemini_session_paths(&project.path, home) {
        let _ = store.load_or_index_file(&conn, "gemini", &file_path, &project, || {
            parse_gemini_history_file(&project, &file_path)
        })?;
    }
    on_progress(0.74, "readingSources");
    for file_path in kiro_session_paths(&project.path, home) {
        let _ = store.load_or_index_file(&conn, "kiro", &file_path, &project, || {
            parse_kiro_history_file(&project, &file_path)
        })?;
    }
    on_progress(0.82, "readingSources");
    for file_path in opencode_history_source_paths(home) {
        let source = if file_path.extension().and_then(|value| value.to_str()) == Some("db") {
            "opencode"
        } else {
            "opencode"
        };
        let _ = store.load_or_index_file(&conn, source, &file_path, &project, || {
            parse_opencode_history_file(&project, &file_path)
        })?;
    }
    on_progress(0.88, "readingSources");
    on_progress(0.96, "aggregating");
    let project_path = project.path.clone();
    let snapshot = store.project_snapshot(&conn, project)?;
    store.save_project_index_state(&conn, &snapshot, &project_path)?;
    Ok(snapshot)
}
