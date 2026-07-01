impl AIUsageStore {
    pub fn default() -> Self {
        Self {
            database_path: default_database_path(),
        }
    }

    pub fn at_path(database_path: PathBuf) -> Self {
        Self { database_path }
    }

    pub fn connect(&self) -> Result<Connection> {
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

    pub fn global_today_normalized_tokens(&self, conn: &Connection) -> Result<i64> {
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

    pub fn global_all_time_normalized_tokens(&self, conn: &Connection) -> Result<i64> {
        conn.query_row(
            "SELECT COALESCE(SUM(total_tokens), 0) FROM ai_history_file_usage_bucket;",
            [],
            |row| row.get(0),
        )
        .map_err(Into::into)
    }

    pub fn indexed_global_project_totals(
        &self,
        conn: &Connection,
    ) -> Result<Vec<AIProjectUsageTotal>> {
        let today_start = local_day_start_seconds(now_seconds());
        let today_end = today_start + 86_400.0;
        let mut statement = conn.prepare(
            r#"
            SELECT
                project.project_id,
                project.project_name,
                project.project_path,
                COUNT(DISTINCT session.source || ':' || COALESCE(session.external_session_id, session.session_key)) AS session_count,
                COALESCE(SUM(bucket.input_tokens), 0) AS input_tokens,
                COALESCE(SUM(bucket.output_tokens), 0) AS output_tokens,
                COALESCE(SUM(bucket.total_tokens), 0) AS total_tokens,
                COALESCE(SUM(bucket.cached_input_tokens), 0) AS cached_input_tokens,
                COALESCE(SUM(bucket.request_count), 0) AS request_count,
                COALESCE((
                    SELECT SUM(active_duration_seconds)
                    FROM (
                        SELECT DISTINCT
                            active_session.source,
                            active_session.file_path,
                            active_session.project_path,
                            active_session.session_key,
                            active_session.active_duration_seconds
                        FROM ai_history_file_session_link AS active_session
                        WHERE active_session.project_id = project.project_id
                          AND active_session.project_path = project.project_path
                    )
                ), 0) AS active_duration_seconds,
                COALESCE(SUM(CASE WHEN bucket.bucket_end > ?1 AND bucket.bucket_start < ?2 THEN bucket.total_tokens ELSE 0 END), 0) AS today_total_tokens,
                COALESCE(SUM(CASE WHEN bucket.bucket_end > ?3 AND bucket.bucket_start < ?4 THEN bucket.cached_input_tokens ELSE 0 END), 0) AS today_cached_input_tokens
            FROM (
                SELECT
                    session.project_id AS project_id,
                    COALESCE(NULLIF(MAX(session.project_name), ''), session.project_path) AS project_name,
                    session.project_path AS project_path
                FROM ai_history_file_session_link AS session
                GROUP BY session.project_id, session.project_path
            ) AS project
            LEFT JOIN ai_history_file_session_link AS session
              ON session.project_id = project.project_id
             AND session.project_path = project.project_path
            LEFT JOIN ai_history_file_usage_bucket AS bucket
              ON bucket.source = session.source
             AND bucket.file_path = session.file_path
             AND bucket.project_path = session.project_path
             AND bucket.session_key = session.session_key
            GROUP BY project.project_id, project.project_name, project.project_path
            ORDER BY total_tokens DESC, project.project_name ASC, project.project_path ASC;
            "#,
        )?;
        let rows = statement
            .query_map(
                params![today_start, today_end, today_start, today_end],
                |row| {
                    Ok(AIProjectUsageTotal {
                        project_id: row.get(0)?,
                        project_name: row.get(1)?,
                        project_path: row.get(2)?,
                        session_count: row.get::<_, i64>(3)?.max(0) as usize,
                        input_tokens: row.get::<_, i64>(4)?.max(0),
                        output_tokens: row.get::<_, i64>(5)?.max(0),
                        total_tokens: row.get::<_, i64>(6)?.max(0),
                        cached_input_tokens: row.get::<_, i64>(7)?.max(0),
                        request_count: row.get::<_, i64>(8)?.max(0),
                        active_duration_seconds: row.get::<_, i64>(9)?.max(0),
                        today_total_tokens: row.get::<_, i64>(10)?.max(0),
                        today_cached_input_tokens: row.get::<_, i64>(11)?.max(0),
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn indexed_global_heatmap(&self, conn: &Connection) -> Result<Vec<AIHeatmapDay>> {
        let input_tokens_expr =
            if usage_bucket_table_has_column(conn, "input_tokens")? {
                "COALESCE(SUM(input_tokens), 0)"
            } else {
                "0"
            };
        let output_tokens_expr =
            if usage_bucket_table_has_column(conn, "output_tokens")? {
                "COALESCE(SUM(output_tokens), 0)"
            } else {
                "0"
            };
        let mut statement = conn.prepare(&format!(
            r#"
            SELECT
                bucket_start,
                {input_tokens_expr} AS input_tokens,
                {output_tokens_expr} AS output_tokens,
                COALESCE(SUM(total_tokens), 0) AS total_tokens,
                COALESCE(SUM(cached_input_tokens), 0) AS cached_input_tokens,
                COALESCE(SUM(request_count), 0) AS request_count
            FROM ai_history_file_usage_bucket
            GROUP BY bucket_start
            ORDER BY bucket_start ASC;
            "#,
        ))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, f64>(0)?,
                    row.get::<_, i64>(1)?.max(0),
                    row.get::<_, i64>(2)?.max(0),
                    row.get::<_, i64>(3)?.max(0),
                    row.get::<_, i64>(4)?.max(0),
                    row.get::<_, i64>(5)?.max(0),
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(heatmap_days_from_buckets(rows))
    }

    pub fn indexed_global_breakdown(
        &self,
        conn: &Connection,
        group_key: &str,
    ) -> Result<Vec<AIUsageBreakdownItem>> {
        let group_expr = match group_key {
            "model" => "COALESCE(NULLIF(bucket.model, ''), 'unknown')",
            _ => "bucket.source",
        };
        let sql = format!(
            r#"
            SELECT
                {group_expr} AS item_key,
                COALESCE(SUM(bucket.total_tokens), 0) AS total_tokens,
                COALESCE(SUM(bucket.cached_input_tokens), 0) AS cached_input_tokens,
                COALESCE(SUM(bucket.request_count), 0) AS request_count
            FROM ai_history_file_usage_bucket AS bucket
            GROUP BY item_key
            ORDER BY total_tokens DESC, item_key ASC;
            "#
        );
        let mut statement = conn.prepare(&sql)?;
        let rows = statement
            .query_map([], |row| {
                Ok(AIUsageBreakdownItem {
                    key: row.get(0)?,
                    total_tokens: row.get::<_, i64>(1)?.max(0),
                    cached_input_tokens: row.get::<_, i64>(2)?.max(0),
                    request_count: row.get::<_, i64>(3)?.max(0),
                    usage_amounts: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn indexed_global_range_summary(
        &self,
        conn: &Connection,
        key: impl Into<String>,
        cutoff: Option<f64>,
    ) -> Result<AIGlobalHistoryRangeSummary> {
        let key = key.into();
        let totals = self.indexed_global_range_totals(conn, cutoff)?;
        let sessions = self.indexed_sessions_since(conn, cutoff)?;
        let project_totals = self.indexed_global_project_totals_since(conn, cutoff)?;
        let tool_breakdown = self.indexed_global_breakdown_since(conn, "source", cutoff)?;
        let model_breakdown = self.indexed_global_breakdown_since(conn, "model", cutoff)?;
        Ok(AIGlobalHistoryRangeSummary {
            key,
            input_tokens: totals.input_tokens,
            output_tokens: totals.output_tokens,
            total_tokens: totals.total_tokens,
            cached_input_tokens: totals.cached_input_tokens,
            request_count: totals.request_count,
            session_count: totals.session_count,
            active_duration_seconds: totals.active_duration_seconds,
            sessions,
            project_totals,
            tool_breakdown,
            model_breakdown,
        })
    }

    pub fn indexed_global_range_totals(
        &self,
        conn: &Connection,
        cutoff: Option<f64>,
    ) -> Result<AIGlobalRangeTotals> {
        conn.query_row(
            r#"
            WITH filtered_buckets AS (
                SELECT *
                FROM ai_history_file_usage_bucket
                WHERE ?1 IS NULL OR bucket_start >= ?2
            ),
            matching_sessions AS (
                SELECT DISTINCT
                    session.source,
                    session.file_path,
                    session.project_path,
                    session.session_key,
                    session.active_duration_seconds
                FROM ai_history_file_session_link AS session
                INNER JOIN filtered_buckets AS bucket
                  ON bucket.source = session.source
                 AND bucket.file_path = session.file_path
                 AND bucket.project_path = session.project_path
                 AND bucket.session_key = session.session_key
            )
            SELECT
                COALESCE((SELECT SUM(input_tokens) FROM filtered_buckets), 0) AS input_tokens,
                COALESCE((SELECT SUM(output_tokens) FROM filtered_buckets), 0) AS output_tokens,
                COALESCE((SELECT SUM(total_tokens) FROM filtered_buckets), 0) AS total_tokens,
                COALESCE((SELECT SUM(cached_input_tokens) FROM filtered_buckets), 0) AS cached_input_tokens,
                COALESCE((SELECT SUM(request_count) FROM filtered_buckets), 0) AS request_count,
                COALESCE((SELECT SUM(active_duration_seconds) FROM matching_sessions), 0) AS active_duration_seconds,
                COALESCE((SELECT COUNT(*) FROM matching_sessions), 0) AS session_count;
            "#,
            params![cutoff, cutoff],
            |row| {
                Ok(AIGlobalRangeTotals {
                    input_tokens: row.get::<_, i64>(0)?.max(0),
                    output_tokens: row.get::<_, i64>(1)?.max(0),
                    total_tokens: row.get::<_, i64>(2)?.max(0),
                    cached_input_tokens: row.get::<_, i64>(3)?.max(0),
                    request_count: row.get::<_, i64>(4)?.max(0),
                    active_duration_seconds: row.get::<_, i64>(5)?.max(0),
                    session_count: row.get::<_, i64>(6)?.max(0) as usize,
                })
            },
        )
        .map_err(Into::into)
    }

    pub fn indexed_global_today_buckets(&self, conn: &Connection) -> Result<Vec<AITimeBucket>> {
        let today_start = local_day_start_seconds(now_seconds());
        let today_end = today_start + 86_400.0;
        self.indexed_global_half_hour_buckets(conn, today_start, today_end)
    }

    pub fn indexed_global_recent_buckets(&self, conn: &Connection) -> Result<Vec<AITimeBucket>> {
        let end = half_hour_bucket_start(now_seconds()) + 30.0 * 60.0;
        let start = end - 48.0 * 60.0 * 60.0;
        self.indexed_global_half_hour_buckets(conn, start, end)
    }

    fn indexed_global_half_hour_buckets(
        &self,
        conn: &Connection,
        range_start: f64,
        range_end: f64,
    ) -> Result<Vec<AITimeBucket>> {
        let mut rows_by_start = HashMap::<i64, AITimeBucket>::new();
        let mut statement = conn.prepare(
            r#"
            SELECT
                bucket_start,
                bucket_end,
                COALESCE(SUM(input_tokens), 0) AS input_tokens,
                COALESCE(SUM(output_tokens), 0) AS output_tokens,
                COALESCE(SUM(total_tokens), 0) AS total_tokens,
                COALESCE(SUM(cached_input_tokens), 0) AS cached_input_tokens,
                COALESCE(SUM(request_count), 0) AS request_count
            FROM ai_history_file_usage_bucket
            WHERE bucket_end > ?1 AND bucket_start < ?2
            GROUP BY bucket_start, bucket_end
            ORDER BY bucket_start ASC;
            "#,
        )?;
        let rows = statement
            .query_map(params![range_start, range_end], |row| {
                Ok(AITimeBucket {
                    start: row.get(0)?,
                    end: row.get(1)?,
                    input_tokens: row.get::<_, i64>(2)?.max(0),
                    output_tokens: row.get::<_, i64>(3)?.max(0),
                    total_tokens: row.get::<_, i64>(4)?.max(0),
                    cached_input_tokens: row.get::<_, i64>(5)?.max(0),
                    request_count: row.get::<_, i64>(6)?.max(0),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for row in rows {
            rows_by_start.insert(row.start as i64, row);
        }
        let bucket_count = (((range_end - range_start) / (30.0 * 60.0)).round() as usize).max(1);
        Ok((0..bucket_count)
            .map(|index| {
                let start = range_start + index as f64 * 30.0 * 60.0;
                rows_by_start.remove(&(start as i64)).unwrap_or(AITimeBucket {
                    start,
                    end: start + 30.0 * 60.0,
                    input_tokens: 0,
                    output_tokens: 0,
                    total_tokens: 0,
                    cached_input_tokens: 0,
                    request_count: 0,
                })
            })
            .collect())
    }

    fn indexed_global_project_totals_since(
        &self,
        conn: &Connection,
        cutoff: Option<f64>,
    ) -> Result<Vec<AIProjectUsageTotal>> {
        let today_start = local_day_start_seconds(now_seconds());
        let today_end = today_start + 86_400.0;
        let mut statement = conn.prepare(
            r#"
            SELECT
                project.project_id,
                project.project_name,
                project.project_path,
                COUNT(DISTINCT CASE WHEN ?1 IS NULL OR bucket.bucket_start >= ?2 THEN session.source || ':' || COALESCE(session.external_session_id, session.session_key) ELSE NULL END) AS session_count,
                COALESCE(SUM(CASE WHEN ?1 IS NULL OR bucket.bucket_start >= ?2 THEN bucket.input_tokens ELSE 0 END), 0) AS input_tokens,
                COALESCE(SUM(CASE WHEN ?3 IS NULL OR bucket.bucket_start >= ?4 THEN bucket.output_tokens ELSE 0 END), 0) AS output_tokens,
                COALESCE(SUM(CASE WHEN ?5 IS NULL OR bucket.bucket_start >= ?6 THEN bucket.total_tokens ELSE 0 END), 0) AS total_tokens,
                COALESCE(SUM(CASE WHEN ?7 IS NULL OR bucket.bucket_start >= ?8 THEN bucket.cached_input_tokens ELSE 0 END), 0) AS cached_input_tokens,
                COALESCE(SUM(CASE WHEN ?9 IS NULL OR bucket.bucket_start >= ?10 THEN bucket.request_count ELSE 0 END), 0) AS request_count,
                COALESCE((
                    SELECT SUM(active_duration_seconds)
                    FROM (
                        SELECT DISTINCT
                            active_session.source,
                            active_session.file_path,
                            active_session.project_path,
                            active_session.session_key,
                            active_session.active_duration_seconds
                        FROM ai_history_file_session_link AS active_session
                        WHERE active_session.project_id = project.project_id
                          AND active_session.project_path = project.project_path
                          AND (?11 IS NULL OR active_session.last_seen_at >= ?12)
                    )
                ), 0) AS active_duration_seconds,
                COALESCE(SUM(CASE WHEN bucket.bucket_end > ?13 AND bucket.bucket_start < ?14 THEN bucket.total_tokens ELSE 0 END), 0) AS today_total_tokens,
                COALESCE(SUM(CASE WHEN bucket.bucket_end > ?15 AND bucket.bucket_start < ?16 THEN bucket.cached_input_tokens ELSE 0 END), 0) AS today_cached_input_tokens
            FROM (
                SELECT
                    session.project_id AS project_id,
                    COALESCE(NULLIF(MAX(session.project_name), ''), session.project_path) AS project_name,
                    session.project_path AS project_path
                FROM ai_history_file_session_link AS session
                WHERE (?11 IS NULL OR session.last_seen_at >= ?12)
                GROUP BY session.project_id, session.project_path
            ) AS project
            LEFT JOIN ai_history_file_session_link AS session
              ON session.project_id = project.project_id
             AND session.project_path = project.project_path
            LEFT JOIN ai_history_file_usage_bucket AS bucket
              ON bucket.source = session.source
             AND bucket.file_path = session.file_path
             AND bucket.project_path = session.project_path
             AND bucket.session_key = session.session_key
            GROUP BY project.project_id, project.project_name, project.project_path
            HAVING total_tokens > 0 OR cached_input_tokens > 0 OR request_count > 0 OR session_count > 0
            ORDER BY total_tokens DESC, project.project_name ASC, project.project_path ASC;
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
                    cutoff,
                    cutoff,
                    today_start,
                    today_end,
                    today_start,
                    today_end
                ],
                |row| {
                    Ok(AIProjectUsageTotal {
                        project_id: row.get(0)?,
                        project_name: row.get(1)?,
                        project_path: row.get(2)?,
                        session_count: row.get::<_, i64>(3)?.max(0) as usize,
                        input_tokens: row.get::<_, i64>(4)?.max(0),
                        output_tokens: row.get::<_, i64>(5)?.max(0),
                        total_tokens: row.get::<_, i64>(6)?.max(0),
                        cached_input_tokens: row.get::<_, i64>(7)?.max(0),
                        request_count: row.get::<_, i64>(8)?.max(0),
                        active_duration_seconds: row.get::<_, i64>(9)?.max(0),
                        today_total_tokens: row.get::<_, i64>(10)?.max(0),
                        today_cached_input_tokens: row.get::<_, i64>(11)?.max(0),
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    fn indexed_global_breakdown_since(
        &self,
        conn: &Connection,
        group_key: &str,
        cutoff: Option<f64>,
    ) -> Result<Vec<AIUsageBreakdownItem>> {
        let group_expr = match group_key {
            "model" => "COALESCE(NULLIF(bucket.model, ''), 'unknown')",
            _ => "bucket.source",
        };
        let sql = format!(
            r#"
            SELECT
                {group_expr} AS item_key,
                COALESCE(SUM(CASE WHEN ?1 IS NULL OR bucket.bucket_start >= ?2 THEN bucket.total_tokens ELSE 0 END), 0) AS total_tokens,
                COALESCE(SUM(CASE WHEN ?3 IS NULL OR bucket.bucket_start >= ?4 THEN bucket.cached_input_tokens ELSE 0 END), 0) AS cached_input_tokens,
                COALESCE(SUM(CASE WHEN ?5 IS NULL OR bucket.bucket_start >= ?6 THEN bucket.request_count ELSE 0 END), 0) AS request_count
            FROM ai_history_file_usage_bucket AS bucket
            GROUP BY item_key
            HAVING total_tokens > 0 OR cached_input_tokens > 0 OR request_count > 0
            ORDER BY total_tokens DESC, item_key ASC;
            "#
        );
        let mut statement = conn.prepare(&sql)?;
        let rows = statement
            .query_map(
                params![cutoff, cutoff, cutoff, cutoff, cutoff, cutoff],
                |row| {
                    Ok(AIUsageBreakdownItem {
                        key: row.get(0)?,
                        total_tokens: row.get::<_, i64>(1)?.max(0),
                        cached_input_tokens: row.get::<_, i64>(2)?.max(0),
                        request_count: row.get::<_, i64>(3)?.max(0),
                        usage_amounts: Vec::new(),
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn normalized_project_totals_since(
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

    pub fn indexed_sessions_since(
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
                        usage_amounts: Vec::new(),
                        today_tokens: row.get(16)?,
                        today_cached_input_tokens: row.get(17)?,
                        today_usage_amounts: Vec::new(),
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

fn usage_bucket_table_has_column(conn: &Connection, column: &str) -> Result<bool> {
    let mut statement = conn.prepare("PRAGMA table_info(ai_history_file_usage_bucket)")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row?.eq_ignore_ascii_case(column) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn heatmap_days_from_buckets(rows: Vec<(f64, i64, i64, i64, i64, i64)>) -> Vec<AIHeatmapDay> {
    let mut days = HashMap::<i64, AIHeatmapDay>::new();
    for (bucket_start, input_tokens, output_tokens, total_tokens, cached_input_tokens, request_count) in rows {
        let day = local_day_start_seconds(bucket_start);
        let item = days.entry(day as i64).or_insert(AIHeatmapDay {
            day,
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            cached_input_tokens: 0,
            request_count: 0,
        });
        item.input_tokens += input_tokens;
        item.output_tokens += output_tokens;
        item.total_tokens += total_tokens;
        item.cached_input_tokens += cached_input_tokens;
        item.request_count += request_count;
    }
    let mut values = days.into_values().collect::<Vec<_>>();
    values.sort_by(|left, right| left.day.total_cmp(&right.day));
    values
}
