fn wait_for_active_workspace_watch_path(service: &RuntimeService, expected: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let (file_path, git_path) = service
            .active_project_watches
            .lock()
            .map(|watches| (watches.file_path.clone(), watches.git_path.clone()))
            .expect("active workspace watches");
        if file_path
            .as_deref()
            .is_some_and(|path| {
                crate::git::repository_path_key(path) == crate::git::repository_path_key(expected)
            })
            && git_path
                .as_deref()
                .is_some_and(|path| {
                    crate::git::repository_path_key(path)
                        == crate::git::repository_path_key(expected)
                })
        {
            return;
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let watches = service
        .active_project_watches
        .lock()
        .expect("active workspace watches");
    assert!(
        watches
            .file_path
            .as_deref()
            .is_some_and(|path| {
                crate::git::repository_path_key(path) == crate::git::repository_path_key(expected)
            })
    );
    assert!(
        watches
            .git_path
            .as_deref()
            .is_some_and(|path| {
                crate::git::repository_path_key(path) == crate::git::repository_path_key(expected)
            })
    );
}

fn wait_for_ai_history_loading_event(
    service: &RuntimeService,
    project_id: &str,
    project_path: &str,
) {
    // Generous cap: the history indexer competes with the whole suite for
    // scheduling; passing runs return on the first matching drain.
    for _ in 0..800 {
        let result = service.drain_ai_history_events();
        if result.events.iter().any(|event| {
            matches!(
                event,
                AIHistoryEvent::ProjectState { state }
                    if state.project_id == project_id
                        && state.project_path == project_path
                        && state.is_loading
            )
        }) {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let result = service.drain_ai_history_events();
    assert!(
        result.events.iter().any(|event| {
            matches!(
                event,
                AIHistoryEvent::ProjectState { state }
                    if state.project_id == project_id
                        && state.project_path == project_path
                        && state.is_loading
            )
        }),
        "expected AI history loading event for {project_id} at {project_path}, got {:?}",
        result.events
    );
}
fn assert_tracked_project_has_git_refresh(service: &RuntimeService, project_id: &str) {
    let activity = service.project_activity_snapshot();
    let tracked = activity
        .tracked_projects
        .iter()
        .find(|project| project.id == project_id)
        .unwrap_or_else(|| panic!("missing tracked project {project_id}: {activity:?}"));
    assert!(
        tracked.has_git_refresh,
        "expected git refresh marker for {project_id}: {activity:?}"
    );
    assert!(
        activity.activated_git_count > 0,
        "expected activated git count after project activation: {activity:?}"
    );
}

#[cfg(unix)]
fn recv_until_contains(rx: &flume::Receiver<Vec<u8>>, needle: &str, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    let mut output = String::new();
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(remaining.min(Duration::from_millis(50))) {
            Ok(bytes) => {
                output.push_str(&String::from_utf8_lossy(&bytes));
                if output.contains(needle) {
                    return output;
                }
            }
            Err(flume::RecvTimeoutError::Timeout) => {}
            Err(flume::RecvTimeoutError::Disconnected) => break,
        }
    }
    output
}

fn write_usage_bucket(
    support_dir: &Path,
    project_dir: &Path,
    project_id: &str,
    project_name: &str,
    session_key: &str,
    total_tokens: i64,
    bucket_start: f64,
) {
    let store = crate::ai_usage_store::AIUsageStore::at_path(support_dir.join("ai-usage.sqlite3"));
    let conn = store.connect().expect("connect ai usage store");
    let project_path = project_dir.to_string_lossy().to_string();
    conn.execute(
        r#"
            INSERT INTO ai_history_file_session_link (
                source, file_path, project_path, session_key, external_session_id, project_id,
                project_name, session_title, first_seen_at, last_seen_at, last_model,
                active_duration_seconds
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
        rusqlite::params![
            "codex",
            "session.jsonl",
            project_path,
            session_key,
            session_key,
            project_id,
            project_name,
            "Session",
            bucket_start,
            bucket_start + 1_800.0,
            "gpt-5",
            60_i64
        ],
    )
    .expect("insert session link");
    conn.execute(
        r#"
            INSERT INTO ai_history_file_usage_bucket (
                source, file_path, project_path, session_key, model, bucket_start, bucket_end,
                input_tokens, output_tokens, total_tokens, cached_input_tokens, request_count,
                active_duration_seconds
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
        rusqlite::params![
            "codex",
            "session.jsonl",
            project_dir.to_string_lossy().to_string(),
            session_key,
            "gpt-5",
            bucket_start,
            bucket_start + 1_800.0,
            total_tokens / 2,
            total_tokens - (total_tokens / 2),
            total_tokens,
            0_i64,
            1_i64,
            60_i64
        ],
    )
    .expect("insert usage bucket");
}
