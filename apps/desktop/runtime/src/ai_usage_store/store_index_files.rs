impl AIUsageStore {
    pub(crate) fn load_or_index_file<F>(
        &self,
        conn: &Connection,
        source: &str,
        file_path: &Path,
        project: &AIHistoryProjectRequest,
        parser: F,
    ) -> Result<AIExternalFileSummary>
    where
        F: FnOnce() -> ParsedHistory,
    {
        let metadata = fs::metadata(file_path)
            .with_context(|| format!("failed to read AI history file {}", file_path.display()))?;
        let normalized_file_path = normalized_path(file_path);
        let modified_at = modified_seconds(&metadata);
        let file_size = metadata.len().min(i64::MAX as u64) as i64;

        if let Some(summary) = self.stored_external_summary(
            conn,
            source,
            &normalized_file_path,
            &project.path,
            Some(modified_at),
        )? {
            return Ok(summary);
        }

        let parsed = parser();
        let summary = external_file_summary_from_parsed(
            source,
            normalized_file_path,
            modified_at,
            file_size,
            project,
            parsed,
        );
        let checkpoint = AIExternalFileCheckpoint {
            source: summary.source.clone(),
            file_path: summary.file_path.clone(),
            project_path: summary.project_path.clone(),
            file_modified_at: summary.file_modified_at,
            file_size: summary.file_size,
            last_offset: summary.file_size,
            last_indexed_at: now_seconds(),
            payload_json: None,
        };
        self.replace_external_summary(conn, &summary, Some(&checkpoint))?;
        Ok(summary)
    }

    pub(crate) fn load_or_index_jsonl_file<AppendParser, RebuildParser>(
        &self,
        conn: &Connection,
        source: &str,
        file_path: &Path,
        project: &AIHistoryProjectRequest,
        append_parser: AppendParser,
        rebuild_parser: RebuildParser,
    ) -> Result<AIExternalFileSummary>
    where
        AppendParser: FnOnce(Option<&AIExternalFileCheckpoint>) -> JSONLParseSnapshot,
        RebuildParser: FnOnce() -> JSONLParseSnapshot,
    {
        let metadata = fs::metadata(file_path)
            .with_context(|| format!("failed to read AI history file {}", file_path.display()))?;
        let normalized_file_path = normalized_path(file_path);
        let modified_at = modified_seconds(&metadata);
        let file_size = metadata.len().min(i64::MAX as u64) as i64;
        let stored_summary =
            self.stored_external_summary(conn, source, &normalized_file_path, &project.path, None)?;
        let checkpoint =
            self.external_file_checkpoint(conn, source, &normalized_file_path, &project.path)?;

        match jsonl_index_mode(
            file_size,
            modified_at,
            stored_summary.as_ref(),
            checkpoint.as_ref(),
        ) {
            JSONLIndexMode::Unchanged => {
                if let Some(summary) = stored_summary {
                    return Ok(summary);
                }
            }
            JSONLIndexMode::Append => {
                if let (Some(stored_summary), Some(checkpoint)) =
                    (stored_summary.as_ref(), checkpoint.as_ref())
                {
                    let snapshot = append_parser(Some(checkpoint));
                    let delta = external_file_summary_from_parsed(
                        source,
                        normalized_file_path.clone(),
                        modified_at,
                        file_size,
                        project,
                        snapshot.result,
                    );
                    let summary = AIExternalFileSummary {
                        source: source.to_string(),
                        file_path: normalized_file_path.clone(),
                        file_modified_at: modified_at,
                        file_size,
                        project_path: project.path.clone(),
                        usage_buckets: merge_usage_buckets(
                            &stored_summary.usage_buckets,
                            &delta.usage_buckets,
                        ),
                    };
                    let checkpoint = AIExternalFileCheckpoint {
                        source: summary.source.clone(),
                        file_path: summary.file_path.clone(),
                        project_path: summary.project_path.clone(),
                        file_modified_at: summary.file_modified_at,
                        file_size: summary.file_size,
                        last_offset: snapshot.last_processed_offset.clamp(0, file_size),
                        last_indexed_at: now_seconds(),
                        payload_json: snapshot
                            .payload_json
                            .or_else(|| checkpoint.payload_json.clone()),
                    };
                    self.replace_external_summary(conn, &summary, Some(&checkpoint))?;
                    return Ok(summary);
                }
            }
            JSONLIndexMode::Rebuild => {}
        }

        let snapshot = rebuild_parser();
        let summary = external_file_summary_from_parsed(
            source,
            normalized_file_path,
            modified_at,
            file_size,
            project,
            snapshot.result,
        );
        let checkpoint = AIExternalFileCheckpoint {
            source: summary.source.clone(),
            file_path: summary.file_path.clone(),
            project_path: summary.project_path.clone(),
            file_modified_at: summary.file_modified_at,
            file_size: summary.file_size,
            last_offset: snapshot.last_processed_offset.clamp(0, file_size),
            last_indexed_at: now_seconds(),
            payload_json: snapshot.payload_json,
        };
        self.replace_external_summary(conn, &summary, Some(&checkpoint))?;
        Ok(summary)
    }

    pub(crate) fn stored_external_summary(
        &self,
        conn: &Connection,
        source: &str,
        file_path: &str,
        project_path: &str,
        modified_at: Option<f64>,
    ) -> Result<Option<AIExternalFileSummary>> {
        let state_modified_at = conn
            .query_row(
                r#"
                SELECT file_modified_at
                FROM ai_history_file_state
                WHERE source = ?1 AND file_path = ?2 AND project_path = ?3
                LIMIT 1;
                "#,
                params![source, file_path, project_path],
                |row| row.get::<_, f64>(0),
            )
            .optional()?;
        let Some(state_modified_at) = state_modified_at else {
            return Ok(None);
        };
        if let Some(modified_at) = modified_at {
            if !same_timestamp(state_modified_at, modified_at) {
                return Ok(None);
            }
        }

        let usage_buckets = self.load_usage_buckets(conn, source, file_path, project_path)?;
        let checkpoint = self.external_file_checkpoint(conn, source, file_path, project_path)?;
        Ok(Some(AIExternalFileSummary {
            source: source.to_string(),
            file_path: file_path.to_string(),
            file_modified_at: state_modified_at,
            file_size: checkpoint.map(|item| item.file_size).unwrap_or(0),
            project_path: project_path.to_string(),
            usage_buckets,
        }))
    }

    pub(crate) fn replace_external_summary(
        &self,
        conn: &Connection,
        summary: &AIExternalFileSummary,
        checkpoint: Option<&AIExternalFileCheckpoint>,
    ) -> Result<()> {
        conn.execute_batch("BEGIN IMMEDIATE TRANSACTION;")?;
        let result = (|| -> Result<()> {
            conn.execute(
                "DELETE FROM ai_history_file_session_link WHERE source = ?1 AND file_path = ?2 AND project_path = ?3;",
                params![summary.source, summary.file_path, summary.project_path],
            )?;
            conn.execute(
                "DELETE FROM ai_history_file_usage_bucket WHERE source = ?1 AND file_path = ?2 AND project_path = ?3;",
                params![summary.source, summary.file_path, summary.project_path],
            )?;
            conn.execute(
                r#"
                INSERT INTO ai_history_file_state (source, file_path, project_path, file_modified_at)
                VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT(source, file_path, project_path) DO UPDATE SET
                    file_modified_at = excluded.file_modified_at;
                "#,
                params![
                    summary.source,
                    summary.file_path,
                    summary.project_path,
                    summary.file_modified_at
                ],
            )?;

            if let Some(checkpoint) = checkpoint {
                conn.execute(
                    r#"
                    INSERT INTO ai_history_file_checkpoint (
                        source, file_path, project_path, file_modified_at,
                        file_size, last_offset, last_indexed_at, payload_json
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                    ON CONFLICT(source, file_path, project_path) DO UPDATE SET
                        file_modified_at = excluded.file_modified_at,
                        file_size = excluded.file_size,
                        last_offset = excluded.last_offset,
                        last_indexed_at = excluded.last_indexed_at,
                        payload_json = excluded.payload_json;
                    "#,
                    params![
                        checkpoint.source,
                        checkpoint.file_path,
                        checkpoint.project_path,
                        checkpoint.file_modified_at,
                        checkpoint.file_size,
                        checkpoint.last_offset,
                        checkpoint.last_indexed_at,
                        checkpoint.payload_json,
                    ],
                )?;
            } else {
                conn.execute(
                    "DELETE FROM ai_history_file_checkpoint WHERE source = ?1 AND file_path = ?2 AND project_path = ?3;",
                    params![summary.source, summary.file_path, summary.project_path],
                )?;
            }

            for session in build_session_links(&summary.usage_buckets) {
                conn.execute(
                    r#"
                    INSERT INTO ai_history_file_session_link (
                        source, file_path, project_path, session_key, external_session_id,
                        project_id, project_name, session_title, first_seen_at, last_seen_at,
                        last_model, active_duration_seconds
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12);
                    "#,
                    params![
                        summary.source,
                        summary.file_path,
                        summary.project_path,
                        session.session_key,
                        session.external_session_id,
                        session.project_id,
                        session.project_name,
                        session.session_title,
                        session.first_seen_at,
                        session.last_seen_at,
                        session.last_model,
                        session.active_duration_seconds,
                    ],
                )?;
            }

            for bucket in &summary.usage_buckets {
                conn.execute(
                    r#"
                    INSERT INTO ai_history_file_usage_bucket (
                        source, file_path, project_path, session_key, model, bucket_start, bucket_end,
                        input_tokens, output_tokens, total_tokens, cached_input_tokens, request_count
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12);
                    "#,
                    params![
                        summary.source,
                        summary.file_path,
                        summary.project_path,
                        bucket.session_key,
                        bucket.model.clone().unwrap_or_default(),
                        bucket.bucket_start,
                        bucket.bucket_end,
                        bucket.input_tokens,
                        bucket.output_tokens,
                        bucket.total_tokens,
                        bucket.cached_input_tokens,
                        bucket.request_count,
                    ],
                )?;
            }
            Ok(())
        })();

        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT;")?;
                Ok(())
            }
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK;");
                Err(error)
            }
        }
    }
}
