impl AIUsageStore {
    pub(crate) fn default() -> Self {
        Self {
            database_path: default_database_path(),
        }
    }

    pub(crate) fn at_path(database_path: PathBuf) -> Self {
        Self { database_path }
    }

    pub(crate) fn connect(&self) -> Result<Connection> {
        if let Some(parent) = self.database_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create AI usage database directory {}",
                    parent.display()
                )
            })?;
        }
        let conn = Connection::open(&self.database_path).with_context(|| {
            format!(
                "failed to open AI usage database {}",
                self.database_path.display()
            )
        })?;
        conn.busy_timeout(std::time::Duration::from_millis(3_000))?;
        initialize_connection(&conn)?;
        Ok(conn)
    }

    pub(crate) fn global_today_normalized_tokens(&self, conn: &Connection) -> Result<i64> {
        let start = local_day_start_seconds(now_seconds());
        let end = start + 86_400.0;
        conn.query_row(
            r#"
            SELECT COALESCE(SUM(total_tokens), 0)
            FROM ai_history_file_usage_bucket
            WHERE bucket_end > ?1 AND bucket_start < ?2;
            "#,
            params![start, end],
            |row| row.get(0),
        )
        .map_err(Into::into)
    }

    pub(crate) fn global_all_time_normalized_tokens(&self, conn: &Connection) -> Result<i64> {
        conn.query_row(
            "SELECT COALESCE(SUM(total_tokens), 0) FROM ai_history_file_usage_bucket;",
            [],
            |row| row.get(0),
        )
        .map_err(Into::into)
    }

    pub(crate) fn normalized_project_totals_since(
        &self,
        conn: &Connection,
        cutoff: Option<f64>,
    ) -> Result<Vec<AIUsageProjectTotal>> {
        let mut statement = conn.prepare(
            r#"
            SELECT
                project_id,
                total_tokens
            FROM (
                SELECT
                    session.project_id AS project_id,
                    COALESCE(SUM(CASE WHEN ?1 IS NULL OR bucket.bucket_start >= ?2 THEN bucket.total_tokens ELSE 0 END), 0) AS total_tokens
                FROM ai_history_file_session_link AS session
                LEFT JOIN ai_history_file_usage_bucket AS bucket
                  ON bucket.source = session.source
                 AND bucket.file_path = session.file_path
                 AND bucket.project_path = session.project_path
                 AND bucket.session_key = session.session_key
                WHERE (?3 IS NULL OR session.last_seen_at >= ?4)
                GROUP BY session.project_id
            )
            WHERE total_tokens > 0
            ORDER BY project_id ASC;
            "#,
        )?;
        let rows = statement
            .query_map(params![cutoff, cutoff, cutoff, cutoff], |row| {
                Ok(AIUsageProjectTotal {
                    project_id: row.get(0)?,
                    total_tokens: row.get::<_, i64>(1)?.max(0),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub(crate) fn indexed_sessions_since(
        &self,
        conn: &Connection,
        cutoff: Option<f64>,
    ) -> Result<Vec<AISessionSummary>> {
        let today_start = local_day_start_seconds(now_seconds());
        let today_end = today_start + 86_400.0;
        let mut statement = conn.prepare(
            r#"
            SELECT
                source,
                session_key,
                external_session_id,
                project_id,
                project_name,
                project_path,
                session_title,
                first_seen_at,
                last_seen_at,
                last_model,
                active_duration_seconds,
                request_count,
                input_tokens,
                output_tokens,
                total_tokens,
                cached_input_tokens,
                today_tokens,
                today_cached_input_tokens
            FROM (
                SELECT
                    session.source AS source,
                    session.session_key AS session_key,
                    session.external_session_id AS external_session_id,
                    session.project_id AS project_id,
                    session.project_name AS project_name,
                    session.project_path AS project_path,
                    session.session_title AS session_title,
                    session.first_seen_at AS first_seen_at,
                    session.last_seen_at AS last_seen_at,
                    session.last_model AS last_model,
                    session.active_duration_seconds AS active_duration_seconds,
                    COALESCE(SUM(CASE WHEN ?1 IS NULL OR bucket.bucket_start >= ?2 THEN bucket.request_count ELSE 0 END), 0) AS request_count,
                    COALESCE(SUM(CASE WHEN ?3 IS NULL OR bucket.bucket_start >= ?4 THEN bucket.input_tokens ELSE 0 END), 0) AS input_tokens,
                    COALESCE(SUM(CASE WHEN ?5 IS NULL OR bucket.bucket_start >= ?6 THEN bucket.output_tokens ELSE 0 END), 0) AS output_tokens,
                    COALESCE(SUM(CASE WHEN ?7 IS NULL OR bucket.bucket_start >= ?8 THEN bucket.total_tokens ELSE 0 END), 0) AS total_tokens,
                    COALESCE(SUM(CASE WHEN ?9 IS NULL OR bucket.bucket_start >= ?10 THEN bucket.cached_input_tokens ELSE 0 END), 0) AS cached_input_tokens,
                    COALESCE(SUM(CASE WHEN bucket.bucket_end > ?11 AND bucket.bucket_start < ?12 THEN bucket.total_tokens ELSE 0 END), 0) AS today_tokens,
                    COALESCE(SUM(CASE WHEN bucket.bucket_end > ?13 AND bucket.bucket_start < ?14 THEN bucket.cached_input_tokens ELSE 0 END), 0) AS today_cached_input_tokens
                FROM ai_history_file_session_link AS session
                LEFT JOIN ai_history_file_usage_bucket AS bucket
                  ON bucket.source = session.source
                 AND bucket.file_path = session.file_path
                 AND bucket.project_path = session.project_path
                 AND bucket.session_key = session.session_key
                WHERE (?15 IS NULL OR session.last_seen_at >= ?16)
                GROUP BY
                    session.source,
                    session.file_path,
                    session.project_path,
                    session.session_key,
                    session.external_session_id,
                    session.project_id,
                    session.project_name,
                    session.session_title,
                    session.first_seen_at,
                    session.last_seen_at,
                    session.last_model,
                    session.active_duration_seconds
            )
            WHERE total_tokens > 0 OR cached_input_tokens > 0 OR request_count > 0
            ORDER BY last_seen_at DESC;
            "#,
        )?;
        let rows = statement
            .query_map(
                params![
                    cutoff,
                    cutoff,
                    cutoff,
                    cutoff,
                    cutoff,
                    cutoff,
                    cutoff,
                    cutoff,
                    cutoff,
                    cutoff,
                    today_start,
                    today_end,
                    today_start,
                    today_end,
                    cutoff,
                    cutoff,
                ],
                |row| {
                    let source: String = row.get(0)?;
                    let session_key: String = row.get(1)?;
                    let external_session_id: Option<String> = row.get(2)?;
                    let first_seen_at: f64 = row.get(7)?;
                    let last_seen_at: f64 = row.get(8)?;
                    let stored_active_duration: i64 = row.get(10)?;
                    let active_duration_seconds = cutoff
                        .map(|cutoff| {
                            if last_seen_at <= cutoff {
                                0
                            } else {
                                let clipped = (last_seen_at - first_seen_at.max(cutoff))
                                    .max(0.0)
                                    .round() as i64;
                                stored_active_duration.max(0).min(clipped)
                            }
                        })
                        .unwrap_or(stored_active_duration.max(0));
                    Ok(AISessionSummary {
                        session_id: deterministic_uuid(&history_group_key(
                            &source,
                            &session_key,
                            external_session_id.as_deref(),
                        )),
                        external_session_id,
                        project_id: row.get(3)?,
                        project_name: row.get(4)?,
                        project_path: row.get(5)?,
                        session_title: row.get(6)?,
                        first_seen_at,
                        last_seen_at,
                        last_tool: Some(source),
                        last_model: row.get(9)?,
                        active_duration_seconds,
                        request_count: row.get(11)?,
                        total_input_tokens: row.get(12)?,
                        total_output_tokens: row.get(13)?,
                        total_tokens: row.get(14)?,
                        cached_input_tokens: row.get(15)?,
                        today_tokens: row.get(16)?,
                        today_cached_input_tokens: row.get(17)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}
