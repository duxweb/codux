pub fn index_project_history_fresh_with_progress<F>(
    project: AIHistoryProjectRequest,
    mut on_progress: F,
) -> AIHistorySnapshot
where
    F: FnMut(f64, &'static str),
{
    index_project_history_fresh_at(project, AIUsageStore::default(), &mut on_progress)
}

pub fn index_project_history_fresh_at<F>(
    project: AIHistoryProjectRequest,
    store: AIUsageStore,
    mut on_progress: F,
) -> AIHistorySnapshot
where
    F: FnMut(f64, &'static str),
{
    load_project_history_with_store_or_fallback(project, &home_dir(), &store, &mut on_progress)
}

pub fn project_history_source_fingerprint(
    project: &AIHistoryProjectRequest,
) -> AIHistorySourceFingerprint {
    let home = home_dir();
    project_history_source_fingerprint_with_home(project, &home)
}

fn project_history_source_fingerprint_with_home(
    project: &AIHistoryProjectRequest,
    home: &Path,
) -> AIHistorySourceFingerprint {
    if project.path.trim().is_empty() {
        return AIHistorySourceFingerprint { files: Vec::new() };
    }
    let mut files = Vec::new();
    for driver in history_sources::history_source_drivers() {
        collect_source_fingerprints(driver.source, driver.paths(project, home), &mut files);
    }
    files.sort_by(|left, right| {
        left.source
            .cmp(&right.source)
            .then_with(|| left.path.cmp(&right.path))
    });
    AIHistorySourceFingerprint { files }
}

fn collect_source_fingerprints(
    source: &str,
    paths: Vec<PathBuf>,
    fingerprints: &mut Vec<AIHistorySourceFileFingerprint>,
) {
    for path in paths {
        for fingerprint_path in sqlite_database_fingerprint_paths(&path) {
            let Ok(metadata) = fs::metadata(&fingerprint_path) else {
                continue;
            };
            fingerprints.push(AIHistorySourceFileFingerprint {
                source: source.to_string(),
                path: normalized_source_path(&fingerprint_path),
                modified_millis: metadata
                    .modified()
                    .ok()
                    .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_millis())
                    .unwrap_or(0),
                size: metadata.len(),
            });
        }
    }
}

fn normalized_source_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn sqlite_database_fingerprint_paths(path: &Path) -> Vec<PathBuf> {
    if !is_sqlite_history_database_path(path) {
        return vec![path.to_path_buf()];
    }
    vec![
        path.to_path_buf(),
        path_with_suffix(path, "-wal"),
        path_with_suffix(path, "-shm"),
    ]
}

pub fn load_indexed_project_history(
    project: AIHistoryProjectRequest,
) -> Result<Option<AIHistorySnapshot>> {
    let store = AIUsageStore::default();
    let conn = store.connect()?;
    store.indexed_project_snapshot(&conn, project)
}

pub fn load_indexed_project_history_at(
    database_path: PathBuf,
    project: AIHistoryProjectRequest,
) -> Result<Option<AIHistorySnapshot>> {
    let store = AIUsageStore::at_path(database_path);
    let conn = store.connect()?;
    store.indexed_project_snapshot(&conn, project)
}

pub fn rename_indexed_history_session(
    project: AIHistoryProjectRequest,
    session_id: String,
    title: String,
) -> Result<Option<AIHistorySnapshot>> {
    let store = AIUsageStore::default();
    rename_indexed_history_session_with_store(store, project, session_id, title)
}

pub fn rename_indexed_history_session_at(
    database_path: PathBuf,
    project: AIHistoryProjectRequest,
    session_id: String,
    title: String,
) -> Result<Option<AIHistorySnapshot>> {
    let store = AIUsageStore::at_path(database_path);
    rename_indexed_history_session_with_store(store, project, session_id, title)
}

fn rename_indexed_history_session_with_store(
    store: AIUsageStore,
    project: AIHistoryProjectRequest,
    session_id: String,
    title: String,
) -> Result<Option<AIHistorySnapshot>> {
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
    remove_indexed_history_session_with_store(store, project, session_id)
}

pub fn remove_indexed_history_session_at(
    database_path: PathBuf,
    project: AIHistoryProjectRequest,
    session_id: String,
) -> Result<Option<AIHistorySnapshot>> {
    let store = AIUsageStore::at_path(database_path);
    remove_indexed_history_session_with_store(store, project, session_id)
}

fn remove_indexed_history_session_with_store(
    store: AIUsageStore,
    project: AIHistoryProjectRequest,
    session_id: String,
) -> Result<Option<AIHistorySnapshot>> {
    let conn = store.connect()?;
    if !store.remove_project_session(&conn, &project.path, &session_id)? {
        return Ok(None);
    }
    store.indexed_project_snapshot(&conn, project)
}

pub fn index_global_history_fresh(
    projects: Vec<AIHistoryProjectRequest>,
) -> AIGlobalHistorySnapshot {
    index_global_history_fresh_at(projects, AIUsageStore::default())
}

pub fn index_global_history_fresh_at(
    projects: Vec<AIHistoryProjectRequest>,
    store: AIUsageStore,
) -> AIGlobalHistorySnapshot {
    let mut total_tokens = 0;
    let mut cached_input_tokens = 0;
    let mut today_total_tokens = 0;
    let mut today_cached_input_tokens = 0;
    let mut project_count = 0;
    let home = home_dir();
    let projects = projects
        .into_iter()
        .filter(|project| !project.path.trim().is_empty())
        .collect::<Vec<_>>();
    let project_paths = projects
        .iter()
        .map(|project| project.path.clone())
        .collect::<Vec<_>>();

    for project in projects {
        let snapshot =
            load_project_history_with_store_or_fallback(project, &home, &store, &mut |_, _| {});
        total_tokens += snapshot.project_summary.project_total_tokens;
        cached_input_tokens += snapshot.project_summary.project_cached_input_tokens;
        today_total_tokens += snapshot.project_summary.today_total_tokens;
        today_cached_input_tokens += snapshot.project_summary.today_cached_input_tokens;
        project_count += 1;
    }

    if let Ok(Some(snapshot)) = load_indexed_global_history_with_store(Some(project_paths), &store) {
        return snapshot;
    }

    AIGlobalHistorySnapshot {
        total_tokens,
        cached_input_tokens,
        today_total_tokens,
        today_cached_input_tokens,
        sessions: Vec::new(),
        project_totals: Vec::new(),
        heatmap: Vec::new(),
        today_time_buckets: Vec::new(),
        recent_time_buckets: Vec::new(),
        tool_breakdown: Vec::new(),
        model_breakdown: Vec::new(),
        range_summaries: Vec::new(),
        project_count,
        indexed_at: now_seconds(),
    }
}

pub fn load_indexed_global_history(
    projects: Vec<AIHistoryProjectRequest>,
) -> Result<Option<AIGlobalHistorySnapshot>> {
    let store = AIUsageStore::default();
    load_indexed_global_history_with_store(Some(project_paths(projects)), &store)
}

pub fn load_indexed_global_history_at(
    database_path: PathBuf,
    projects: Vec<AIHistoryProjectRequest>,
) -> Result<Option<AIGlobalHistorySnapshot>> {
    let store = AIUsageStore::at_path(database_path);
    load_indexed_global_history_with_store(Some(project_paths(projects)), &store)
}

pub fn load_all_indexed_global_history_at(
    database_path: PathBuf,
) -> Result<Option<AIGlobalHistorySnapshot>> {
    let store = AIUsageStore::at_path(database_path);
    load_indexed_global_history_with_store(None, &store)
}

fn load_indexed_global_history_with_store(
    project_paths: Option<Vec<String>>,
    store: &AIUsageStore,
) -> Result<Option<AIGlobalHistorySnapshot>> {
    let conn = store.connect()?;
    let now = now_seconds();
    if let Some(project_paths) = project_paths.as_ref() {
        store.retain_project_paths(&conn, project_paths)?;
    }
    let mut project_totals = store.indexed_global_project_totals(&conn)?;
    if let Some(project_paths) = project_paths.as_ref() {
        let requested_paths = project_paths
            .iter()
            .filter_map(|path| normalized_history_path(path))
            .collect::<HashSet<_>>();
        let mut statement = conn.prepare("SELECT project_path FROM ai_history_project_index_state;")?;
        let indexed_paths = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter_map(|path| normalized_history_path(&path))
            .collect::<HashSet<_>>();
        if indexed_paths != requested_paths {
            return Ok(None);
        }
    }
    let sessions = store.indexed_sessions_since(&conn, None)?;
    for project in &mut project_totals {
        project.active_duration_seconds = sessions
            .iter()
            .filter(|session| {
                session.project_id == project.project_id
                    && session.project_path == project.project_path
            })
            .map(|session| session.active_duration_seconds)
            .sum();
    }
    let (total_tokens, cached_input_tokens, today_total_tokens, today_cached_input_tokens) =
        project_totals
            .iter()
            .fold((0, 0, 0, 0), |(total, cached, today, today_cached), project| {
                (
                    total + project.total_tokens,
                    cached + project.cached_input_tokens,
                    today + project.today_total_tokens,
                    today_cached + project.today_cached_input_tokens,
                )
            });
    let range_summaries = indexed_global_range_summaries(store, &conn, now)?;
    let project_count = project_totals.len();
    Ok(Some(AIGlobalHistorySnapshot {
        total_tokens,
        cached_input_tokens,
        today_total_tokens,
        today_cached_input_tokens,
        sessions,
        project_totals,
        heatmap: store.indexed_global_heatmap(&conn)?,
        today_time_buckets: store.indexed_global_today_buckets(&conn)?,
        recent_time_buckets: store.indexed_global_recent_buckets(&conn)?,
        tool_breakdown: store.indexed_global_breakdown(&conn, "source")?,
        model_breakdown: store.indexed_global_breakdown(&conn, "model")?,
        range_summaries,
        project_count,
        indexed_at: now,
    }))
}

fn project_paths(projects: Vec<AIHistoryProjectRequest>) -> Vec<String> {
    projects
        .into_iter()
        .filter_map(|project| {
            let path = project.path.trim();
            (!path.is_empty()).then(|| path.to_string())
        })
        .collect()
}

fn indexed_global_range_summaries(
    store: &AIUsageStore,
    conn: &Connection,
    now: f64,
) -> Result<Vec<AIGlobalHistoryRangeSummary>> {
    let today = local_day_start_seconds(now);
    let ranges = [
        ("today", Some(today)),
        ("sevenDays", Some(now - 7.0 * 86_400.0)),
        ("thirtyDays", Some(now - 30.0 * 86_400.0)),
        ("all", None),
    ];
    ranges
        .into_iter()
        .map(|(key, cutoff)| store.indexed_global_range_summary(conn, key, cutoff))
        .collect()
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

pub fn global_all_time_normalized_tokens_at(database_path: PathBuf) -> Result<i64> {
    let store = AIUsageStore::at_path(database_path);
    let conn = store.connect()?;
    store.global_all_time_normalized_tokens(&conn)
}

pub fn indexed_sessions_since(cutoff: Option<f64>) -> Result<Vec<AISessionSummary>> {
    let store = AIUsageStore::default();
    let conn = store.connect()?;
    store.indexed_sessions_since(&conn, cutoff)
}

pub fn indexed_sessions_since_at(
    database_path: PathBuf,
    cutoff: Option<f64>,
) -> Result<Vec<AISessionSummary>> {
    let store = AIUsageStore::at_path(database_path);
    let conn = store.connect()?;
    store.indexed_sessions_since(&conn, cutoff)
}

pub fn normalized_project_totals_since(
    cutoff: Option<f64>,
) -> Result<Vec<crate::usage_store::AIUsageProjectTotal>> {
    let store = AIUsageStore::default();
    let conn = store.connect()?;
    store.normalized_project_totals_since(&conn, cutoff)
}

pub fn normalized_project_totals_since_at(
    database_path: PathBuf,
    cutoff: Option<f64>,
) -> Result<Vec<crate::usage_store::AIUsageProjectTotal>> {
    let store = AIUsageStore::at_path(database_path);
    let conn = store.connect()?;
    store.normalized_project_totals_since(&conn, cutoff)
}

fn load_project_history_with_store_or_fallback(
    project: AIHistoryProjectRequest,
    home: &Path,
    store: &AIUsageStore,
    on_progress: &mut impl FnMut(f64, &'static str),
) -> AIHistorySnapshot {
    if project.path.trim().is_empty() {
        return build_snapshot(project, ParsedHistory::default());
    }

    on_progress(0.12, "readingSources");
    if let Ok(snapshot) = load_project_history_with_store(project.clone(), home, store, on_progress)
    {
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
    for driver in history_sources::history_source_drivers() {
        parsed.merge(driver.parse_all(&project, home));
        on_progress(history_sources::history_source_progress(driver.source), "readingSources");
    }
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
    for driver in history_sources::history_source_drivers() {
        driver.load_or_index(store, &conn, &project, home)?;
        on_progress(history_sources::history_source_progress(driver.source), "readingSources");
    }
    on_progress(0.96, "aggregating");
    let project_path = project.path.clone();
    let snapshot = store.project_snapshot(&conn, project)?;
    store.save_project_index_state(&conn, &snapshot, &project_path)?;
    Ok(snapshot)
}
